//! Storage layer — SQLite persistence and vector index.

pub mod sqlite;
pub mod vector;

use crate::memory::types::{KnowledgeGraphUpdate, MemoryRecord, MemoryType};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

// Re-export async_trait for downstream use (it's a dev convenience)
pub use async_trait::async_trait as storage_trait;

/// Trait for memory persistence backends.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Insert a new memory record.
    async fn insert(&self, record: &MemoryRecord) -> anyhow::Result<()>;

    /// Get a memory by ID.
    async fn get(&self, id: Uuid) -> anyhow::Result<Option<MemoryRecord>>;

    /// Update an existing memory record.
    async fn update(&self, record: &MemoryRecord) -> anyhow::Result<()>;

    /// Delete a memory by ID.
    async fn delete(&self, id: Uuid) -> anyhow::Result<bool>;

    async fn insert_knowledge_triple(
        &self,
        triple: &KnowledgeGraphUpdate,
        memory_id: Uuid,
    ) -> anyhow::Result<()>;

    /// Get all knowledge triples related to an entity (subject or object).
    async fn get_knowledge_for_entity(
        &self,
        entity: &str,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>>;

    /// Query knowledge triples with optional filters.
    async fn query_knowledge(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>>;

    /// List recent entity names for resolution purposes.
    async fn list_recent_entities(&self, limit: usize) -> anyhow::Result<Vec<String>>;

    /// Full-text search using SQLite FTS5.
    async fn search_fts(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryRecord>>;

    /// List memories with optional filters.
    async fn list(
        &self,
        filter_tags: &[String],
        memory_type: Option<MemoryType>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryRecord>>;

    /// Get database statistics.
    async fn stats(&self) -> anyhow::Result<StoreStats>;

    /// Archive a memory (soft delete with decay).
    async fn archive(&self, id: Uuid) -> anyhow::Result<bool>;

    /// Apply decay to all memories based on importance weighting.
    async fn apply_decay(&self, decay_factor: f32) -> anyhow::Result<usize>;

    /// Get list of memory IDs that have decayed below the threshold.
    async fn get_decayed_ids(&self, threshold: f32) -> anyhow::Result<Vec<Uuid>>;
}

/// Database statistics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct StoreStats {
    pub total_memories: usize,
    pub by_type: std::collections::HashMap<String, usize>,
    pub avg_importance: f32,
    pub db_size_bytes: u64,
}
