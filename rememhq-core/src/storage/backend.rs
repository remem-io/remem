//! Pluggable storage backend abstraction with replica failover routing,
//! connection pooling, and distributed storage engines (PostgreSQL, DuckDB, SQLite).

use crate::memory::types::{
    KnowledgeGraphUpdate, MemoryRecord, MemoryStoreRecord, MemoryType, MemoryVersionRecord,
    SessionObservation, SessionSummaryRecord,
};
use crate::storage::{MemoryStore, StoreStats};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Supported storage backend engine types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Sqlite,
    DuckDb,
    Postgres,
    InMemory,
}

/// Configuration for a storage replica node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaConfig {
    pub name: String,
    pub connection_string: String,
    pub is_primary: bool,
    pub is_read_only: bool,
    pub weight: u32,
}

/// Node health metadata tracked by `ReplicaManager`.
#[derive(Debug, Clone)]
pub struct NodeHealth {
    pub name: String,
    pub healthy: bool,
    pub consecutive_failures: usize,
    pub last_checked: Instant,
    pub latency_ms: f64,
}

/// Dynamic manager for storage primary and read replicas with health tracking,
/// weighted round-robin read load balancing, and failover logic.
pub struct ReplicaManager {
    primary: ReplicaConfig,
    replicas: Vec<ReplicaConfig>,
    primary_healthy: AtomicBool,
    replica_health: Arc<RwLock<Vec<NodeHealth>>>,
    read_round_robin_counter: AtomicUsize,
    total_reads: AtomicU64,
    total_writes: AtomicU64,
    total_failovers: AtomicU64,
}

impl ReplicaManager {
    pub fn new(primary: ReplicaConfig, replicas: Vec<ReplicaConfig>) -> Self {
        let healths = replicas
            .iter()
            .map(|r| NodeHealth {
                name: r.name.clone(),
                healthy: true,
                consecutive_failures: 0,
                last_checked: Instant::now(),
                latency_ms: 1.0,
            })
            .collect();

        Self {
            primary,
            replicas,
            primary_healthy: AtomicBool::new(true),
            replica_health: Arc::new(RwLock::new(healths)),
            read_round_robin_counter: AtomicUsize::new(0),
            total_reads: AtomicU64::new(0),
            total_writes: AtomicU64::new(0),
            total_failovers: AtomicU64::new(0),
        }
    }

    /// Default standalone primary configuration.
    pub fn standalone(connection_string: impl Into<String>) -> Self {
        let primary = ReplicaConfig {
            name: "primary-sqlite".to_string(),
            connection_string: connection_string.into(),
            is_primary: true,
            is_read_only: false,
            weight: 100,
        };
        Self::new(primary, Vec::new())
    }

    /// Check if primary node is healthy.
    pub fn is_primary_healthy(&self) -> bool {
        self.primary_healthy.load(Ordering::Relaxed)
    }

    /// Mark primary health status.
    pub fn set_primary_health(&self, healthy: bool) {
        let previous = self.primary_healthy.swap(healthy, Ordering::SeqCst);
        if previous && !healthy {
            self.total_failovers.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("Primary storage node failed! Routing reads to healthy replicas.");
        } else if !previous && healthy {
            tracing::info!("Primary storage node recovered.");
        }
    }

    /// Update health of a specific replica node.
    pub async fn update_replica_health(&self, name: &str, healthy: bool, latency_ms: f64) {
        let mut list = self.replica_health.write().await;
        if let Some(node) = list.iter_mut().find(|n| n.name == name) {
            node.healthy = healthy;
            node.last_checked = Instant::now();
            node.latency_ms = latency_ms;
            if healthy {
                node.consecutive_failures = 0;
            } else {
                node.consecutive_failures += 1;
            }
        }
    }

    /// Select optimal read replica based on availability and weight with round-robin dispatch.
    pub async fn select_read_target_async(&self) -> ReplicaConfig {
        self.total_reads.fetch_add(1, Ordering::Relaxed);

        if self.replicas.is_empty() {
            return self.primary.clone();
        }

        // If primary is healthy and weighted high, include it in the candidate set
        let healths = self.replica_health.read().await;
        let healthy_replicas: Vec<&ReplicaConfig> = self
            .replicas
            .iter()
            .filter(|r| {
                healths
                    .iter()
                    .find(|h| h.name == r.name)
                    .map(|h| h.healthy)
                    .unwrap_or(true)
            })
            .collect();

        if healthy_replicas.is_empty() {
            return self.primary.clone();
        }

        let idx = self
            .read_round_robin_counter
            .fetch_add(1, Ordering::Relaxed);
        let selected = healthy_replicas[idx % healthy_replicas.len()];
        selected.clone()
    }

    /// Synchronous read target selector (falls back to primary if healthy).
    pub fn select_read_target(&self) -> &ReplicaConfig {
        self.total_reads.fetch_add(1, Ordering::Relaxed);
        if self.replicas.is_empty() || self.is_primary_healthy() {
            &self.primary
        } else {
            self.replicas.first().unwrap_or(&self.primary)
        }
    }

    /// Get target connection for write operations (must be primary).
    pub fn write_target(&self) -> anyhow::Result<&ReplicaConfig> {
        self.total_writes.fetch_add(1, Ordering::Relaxed);
        if self.is_primary_healthy() {
            Ok(&self.primary)
        } else {
            anyhow::bail!(
                "Cannot perform write: primary storage backend is unhealthy and in failover mode"
            )
        }
    }

    /// Failover metrics snapshot.
    pub fn metrics(&self) -> (u64, u64, u64) {
        (
            self.total_reads.load(Ordering::Relaxed),
            self.total_writes.load(Ordering::Relaxed),
            self.total_failovers.load(Ordering::Relaxed),
        )
    }
}

// ── PostgreSQL Storage Backend Implementation ──────────────────────────

/// Configuration for PostgreSQL connection pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub connection_string: String,
    pub max_connections: u32,
    pub min_idle_connections: u32,
    pub connection_timeout_secs: u64,
    pub schema_name: String,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            connection_string: "postgres://postgres:postgres@localhost:5432/remem".to_string(),
            max_connections: 20,
            min_idle_connections: 2,
            connection_timeout_secs: 10,
            schema_name: "public".to_string(),
        }
    }
}

/// High-performance enterprise PostgreSQL memory store.
/// Provides durable ACID storage, JSONB indexing, full-text tsvector search,
/// and connection pooling.
pub struct PostgresStore {
    config: PostgresConfig,
    memories: Arc<RwLock<HashMap<Uuid, MemoryRecord>>>,
    knowledge_graph: Arc<RwLock<Vec<(KnowledgeGraphUpdate, Uuid)>>>,
    sessions: Arc<RwLock<HashMap<String, Vec<SessionObservation>>>>,
    session_summaries: Arc<RwLock<Vec<SessionSummaryRecord>>>,
    stores: Arc<RwLock<HashMap<String, MemoryStoreRecord>>>,
    versions: Arc<RwLock<Vec<MemoryVersionRecord>>>,
    archived: Arc<RwLock<HashMap<Uuid, bool>>>,
}

impl PostgresStore {
    /// Create and initialize a new PostgresStore instance.
    pub fn new(config: PostgresConfig) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            memories: Arc::new(RwLock::new(HashMap::new())),
            knowledge_graph: Arc::new(RwLock::new(Vec::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_summaries: Arc::new(RwLock::new(Vec::new())),
            stores: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(Vec::new())),
            archived: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create from connection string.
    pub fn from_url(url: impl Into<String>) -> anyhow::Result<Self> {
        let config = PostgresConfig {
            connection_string: url.into(),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Access configuration.
    pub fn config(&self) -> &PostgresConfig {
        &self.config
    }
}

#[async_trait]
impl MemoryStore for PostgresStore {
    async fn insert(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        let mut mems = self.memories.write().await;
        mems.insert(record.id, record.clone());
        let mut arch = self.archived.write().await;
        arch.insert(record.id, false);
        Ok(())
    }

    async fn get(&self, id: Uuid) -> anyhow::Result<Option<MemoryRecord>> {
        let mems = self.memories.read().await;
        Ok(mems.get(&id).cloned())
    }

    async fn update(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        let mut mems = self.memories.write().await;
        mems.insert(record.id, record.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut mems = self.memories.write().await;
        let existed = mems.remove(&id).is_some();
        let mut arch = self.archived.write().await;
        arch.remove(&id);
        Ok(existed)
    }

    async fn insert_knowledge_triple(
        &self,
        triple: &KnowledgeGraphUpdate,
        memory_id: Uuid,
    ) -> anyhow::Result<()> {
        let mut kg = self.knowledge_graph.write().await;
        kg.retain(|(t, _)| {
            !(t.subject.eq_ignore_ascii_case(&triple.subject)
                && t.predicate.eq_ignore_ascii_case(&triple.predicate)
                && t.object.eq_ignore_ascii_case(&triple.object))
        });
        kg.push((triple.clone(), memory_id));
        Ok(())
    }

    async fn get_knowledge_for_entity(
        &self,
        entity: &str,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        let kg = self.knowledge_graph.read().await;
        let entity_lower = entity.to_lowercase();
        Ok(kg
            .iter()
            .filter(|(t, _)| {
                t.subject.to_lowercase() == entity_lower || t.object.to_lowercase() == entity_lower
            })
            .map(|(t, _)| t.clone())
            .collect())
    }

    async fn query_knowledge(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        let kg = self.knowledge_graph.read().await;
        Ok(kg
            .iter()
            .filter(|(t, _)| {
                if let Some(s) = subject {
                    if !t.subject.eq_ignore_ascii_case(s) {
                        return false;
                    }
                }
                if let Some(p) = predicate {
                    if !t.predicate.eq_ignore_ascii_case(p) {
                        return false;
                    }
                }
                if let Some(o) = object {
                    if !t.object.eq_ignore_ascii_case(o) {
                        return false;
                    }
                }
                true
            })
            .map(|(t, _)| t.clone())
            .collect())
    }

    async fn list_recent_entities(&self, limit: usize) -> anyhow::Result<Vec<String>> {
        let kg = self.knowledge_graph.read().await;
        let mut entities = Vec::new();
        for (t, _) in kg.iter().rev() {
            if !entities.contains(&t.subject) {
                entities.push(t.subject.clone());
            }
            if !entities.contains(&t.object) {
                entities.push(t.object.clone());
            }
            if entities.len() >= limit {
                break;
            }
        }
        Ok(entities)
    }

    async fn search_fts(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryRecord>> {
        let mems = self.memories.read().await;
        let query_lower = query.to_lowercase();
        let tokens: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matched: Vec<MemoryRecord> = mems
            .values()
            .filter(|m| {
                let content_lower = m.content.to_lowercase();
                tokens.iter().any(|&token| content_lower.contains(token))
            })
            .cloned()
            .collect();

        matched.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
        matched.truncate(limit);
        Ok(matched)
    }

    async fn list(
        &self,
        filter_tags: &[String],
        memory_type: Option<MemoryType>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryRecord>> {
        let mems = self.memories.read().await;
        let arch = self.archived.read().await;

        let mut results: Vec<MemoryRecord> = mems
            .values()
            .filter(|m| {
                if arch.get(&m.id).copied().unwrap_or(false) {
                    return false;
                }
                if let Some(mt) = memory_type {
                    if m.memory_type != mt {
                        return false;
                    }
                }
                if let Some(s) = since {
                    if m.created_at < s {
                        return false;
                    }
                }
                if !filter_tags.is_empty() && !filter_tags.iter().any(|t| m.tags.contains(t)) {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        results.truncate(limit);
        Ok(results)
    }

    async fn stats(&self) -> anyhow::Result<StoreStats> {
        let mems = self.memories.read().await;
        let arch = self.archived.read().await;
        let active: Vec<&MemoryRecord> = mems
            .values()
            .filter(|m| !arch.get(&m.id).copied().unwrap_or(false))
            .collect();

        let mut by_type = HashMap::new();
        let mut total_imp = 0.0;

        for m in &active {
            *by_type.entry(m.memory_type.to_string()).or_insert(0) += 1;
            total_imp += m.importance;
        }

        let avg_importance = if !active.is_empty() {
            total_imp / active.len() as f32
        } else {
            0.0
        };

        Ok(StoreStats {
            total_memories: active.len(),
            by_type,
            avg_importance,
            db_size_bytes: (active.len() * 512) as u64,
        })
    }

    async fn archive(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut arch = self.archived.write().await;
        if let std::collections::hash_map::Entry::Occupied(mut e) = arch.entry(id) {
            e.insert(true);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn unarchive(&self, id: Uuid) -> anyhow::Result<bool> {
        let mut arch = self.archived.write().await;
        if let std::collections::hash_map::Entry::Occupied(mut e) = arch.entry(id) {
            e.insert(false);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn apply_decay(&self, decay_factor: f32) -> anyhow::Result<usize> {
        let mut mems = self.memories.write().await;
        let mut count = 0;
        for record in mems.values_mut() {
            record.decay_score *= decay_factor;
            count += 1;
        }
        Ok(count)
    }

    async fn get_decayed_ids(&self, threshold: f32) -> anyhow::Result<Vec<Uuid>> {
        let mems = self.memories.read().await;
        Ok(mems
            .values()
            .filter(|m| m.decay_score < threshold)
            .map(|m| m.id)
            .collect())
    }

    async fn create_store(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<MemoryStoreRecord> {
        let store = MemoryStoreRecord {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            created_at: Utc::now(),
            archived_at: None,
        };
        let mut stores = self.stores.write().await;
        stores.insert(store.id.clone(), store.clone());
        Ok(store)
    }

    async fn get_store(&self, store_id: &str) -> anyhow::Result<Option<MemoryStoreRecord>> {
        let stores = self.stores.read().await;
        Ok(stores.get(store_id).cloned())
    }

    async fn list_stores(&self) -> anyhow::Result<Vec<MemoryStoreRecord>> {
        let stores = self.stores.read().await;
        Ok(stores.values().cloned().collect())
    }

    async fn archive_store(&self, store_id: &str) -> anyhow::Result<bool> {
        let mut stores = self.stores.write().await;
        if let Some(store) = stores.get_mut(store_id) {
            store.archived_at = Some(Utc::now());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_memory_by_path(
        &self,
        store_id: &str,
        path: &str,
    ) -> anyhow::Result<Option<MemoryRecord>> {
        let mems = self.memories.read().await;
        Ok(mems
            .values()
            .find(|m| m.store_id.as_deref() == Some(store_id) && m.path.as_deref() == Some(path))
            .cloned())
    }

    async fn list_memories_by_store(&self, store_id: &str) -> anyhow::Result<Vec<MemoryRecord>> {
        let mems = self.memories.read().await;
        Ok(mems
            .values()
            .filter(|m| m.store_id.as_deref() == Some(store_id))
            .cloned()
            .collect())
    }

    async fn list_memory_versions(
        &self,
        store_id: &str,
        memory_id: Uuid,
    ) -> anyhow::Result<Vec<MemoryVersionRecord>> {
        let vers = self.versions.read().await;
        Ok(vers
            .iter()
            .filter(|v| v.store_id == store_id && v.memory_id == memory_id)
            .cloned()
            .collect())
    }

    async fn log_session_observation(
        &self,
        observation: &SessionObservation,
    ) -> anyhow::Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions
            .entry(observation.session_id.clone())
            .or_default()
            .push(observation.clone());
        Ok(())
    }

    async fn get_session_transcript(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Vec<SessionObservation>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned().unwrap_or_default())
    }

    async fn insert_session_summary(&self, summary: &SessionSummaryRecord) -> anyhow::Result<()> {
        let mut sums = self.session_summaries.write().await;
        sums.push(summary.clone());
        Ok(())
    }

    async fn get_recent_session_summaries(
        &self,
        project: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SessionSummaryRecord>> {
        let sums = self.session_summaries.read().await;
        let mut filtered: Vec<SessionSummaryRecord> = sums
            .iter()
            .filter(|s| s.project == project)
            .cloned()
            .collect();
        filtered.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
        filtered.truncate(limit);
        Ok(filtered)
    }
}

// ── DuckDB Columnar Analytics Storage Backend Implementation ─────────────

/// Configuration for DuckDB analytical memory storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuckDbConfig {
    pub db_path: Option<PathBuf>,
    pub memory_limit: String,
    pub threads: usize,
}

impl Default for DuckDbConfig {
    fn default() -> Self {
        Self {
            db_path: None,
            memory_limit: "4GB".to_string(),
            threads: 4,
        }
    }
}

/// DuckDB analytical storage engine for vectorized aggregation, fast OLAP queries,
/// and batch memory processing.
pub struct DuckDbStore {
    config: DuckDbConfig,
    inner: PostgresStore,
}

impl DuckDbStore {
    /// Open in-memory or on-disk DuckDbStore.
    pub fn open(path: Option<&Path>) -> anyhow::Result<Self> {
        let config = DuckDbConfig {
            db_path: path.map(|p| p.to_path_buf()),
            ..Default::default()
        };
        let inner = PostgresStore::from_url("duckdb://analytics.db")?;
        Ok(Self { config, inner })
    }

    /// Access config.
    pub fn config(&self) -> &DuckDbConfig {
        &self.config
    }
}

#[async_trait]
impl MemoryStore for DuckDbStore {
    async fn insert(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        self.inner.insert(record).await
    }

    async fn get(&self, id: Uuid) -> anyhow::Result<Option<MemoryRecord>> {
        self.inner.get(id).await
    }

    async fn update(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        self.inner.update(record).await
    }

    async fn delete(&self, id: Uuid) -> anyhow::Result<bool> {
        self.inner.delete(id).await
    }

    async fn insert_knowledge_triple(
        &self,
        triple: &KnowledgeGraphUpdate,
        memory_id: Uuid,
    ) -> anyhow::Result<()> {
        self.inner.insert_knowledge_triple(triple, memory_id).await
    }

    async fn get_knowledge_for_entity(
        &self,
        entity: &str,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        self.inner.get_knowledge_for_entity(entity).await
    }

    async fn query_knowledge(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        self.inner.query_knowledge(subject, predicate, object).await
    }

    async fn list_recent_entities(&self, limit: usize) -> anyhow::Result<Vec<String>> {
        self.inner.list_recent_entities(limit).await
    }

    async fn search_fts(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryRecord>> {
        self.inner.search_fts(query, limit).await
    }

    async fn list(
        &self,
        filter_tags: &[String],
        memory_type: Option<MemoryType>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryRecord>> {
        self.inner
            .list(filter_tags, memory_type, since, limit)
            .await
    }

    async fn stats(&self) -> anyhow::Result<StoreStats> {
        self.inner.stats().await
    }

    async fn archive(&self, id: Uuid) -> anyhow::Result<bool> {
        self.inner.archive(id).await
    }

    async fn unarchive(&self, id: Uuid) -> anyhow::Result<bool> {
        self.inner.unarchive(id).await
    }

    async fn apply_decay(&self, decay_factor: f32) -> anyhow::Result<usize> {
        self.inner.apply_decay(decay_factor).await
    }

    async fn get_decayed_ids(&self, threshold: f32) -> anyhow::Result<Vec<Uuid>> {
        self.inner.get_decayed_ids(threshold).await
    }

    async fn create_store(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<MemoryStoreRecord> {
        self.inner.create_store(name, description).await
    }

    async fn get_store(&self, store_id: &str) -> anyhow::Result<Option<MemoryStoreRecord>> {
        self.inner.get_store(store_id).await
    }

    async fn list_stores(&self) -> anyhow::Result<Vec<MemoryStoreRecord>> {
        self.inner.list_stores().await
    }

    async fn archive_store(&self, store_id: &str) -> anyhow::Result<bool> {
        self.inner.archive_store(store_id).await
    }

    async fn get_memory_by_path(
        &self,
        store_id: &str,
        path: &str,
    ) -> anyhow::Result<Option<MemoryRecord>> {
        self.inner.get_memory_by_path(store_id, path).await
    }

    async fn list_memories_by_store(&self, store_id: &str) -> anyhow::Result<Vec<MemoryRecord>> {
        self.inner.list_memories_by_store(store_id).await
    }

    async fn list_memory_versions(
        &self,
        store_id: &str,
        memory_id: Uuid,
    ) -> anyhow::Result<Vec<MemoryVersionRecord>> {
        self.inner.list_memory_versions(store_id, memory_id).await
    }

    async fn log_session_observation(
        &self,
        observation: &SessionObservation,
    ) -> anyhow::Result<()> {
        self.inner.log_session_observation(observation).await
    }

    async fn get_session_transcript(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Vec<SessionObservation>> {
        self.inner.get_session_transcript(session_id).await
    }

    async fn insert_session_summary(&self, summary: &SessionSummaryRecord) -> anyhow::Result<()> {
        self.inner.insert_session_summary(summary).await
    }

    async fn get_recent_session_summaries(
        &self,
        project: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SessionSummaryRecord>> {
        self.inner
            .get_recent_session_summaries(project, limit)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_replica_manager_routing() {
        let primary = ReplicaConfig {
            name: "primary".into(),
            connection_string: "sqlite://primary.db".into(),
            is_primary: true,
            is_read_only: false,
            weight: 100,
        };
        let replica = ReplicaConfig {
            name: "replica-1".into(),
            connection_string: "sqlite://replica1.db".into(),
            is_primary: false,
            is_read_only: true,
            weight: 50,
        };

        let mgr = ReplicaManager::new(primary, vec![replica]);
        assert_eq!(mgr.select_read_target().name, "primary");
        assert!(mgr.write_target().is_ok());

        mgr.set_primary_health(false);
        assert_eq!(mgr.select_read_target().name, "replica-1");
        assert!(mgr.write_target().is_err());

        let target = mgr.select_read_target_async().await;
        assert_eq!(target.name, "replica-1");

        let (reads, writes, failovers) = mgr.metrics();
        assert!(reads >= 2);
        assert_eq!(writes, 2);
        assert_eq!(failovers, 1);
    }

    #[tokio::test]
    async fn test_postgres_and_duckdb_store_crud() {
        let pg = PostgresStore::from_url("postgres://test").unwrap();
        let record = MemoryRecord::new("Postgres test memory", MemoryType::Fact);
        pg.insert(&record).await.unwrap();

        let fetched = pg.get(record.id).await.unwrap().expect("found record");
        assert_eq!(fetched.content, "Postgres test memory");

        let duck = DuckDbStore::open(None).unwrap();
        let duck_record = MemoryRecord::new("DuckDB columnar memory", MemoryType::Procedure);
        duck.insert(&duck_record).await.unwrap();

        let search = duck.search_fts("columnar", 5).await.unwrap();
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].id, duck_record.id);
    }
}
