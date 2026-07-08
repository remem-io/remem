//! Reasoning engine — the core differentiator of remem.
//!
//! Uses cloud LLMs to add intelligence to every memory operation:
//! scoring, guided retrieval, consolidation, and contradiction detection.

pub mod compaction;
pub mod consolidation;
pub mod contradiction;
pub mod resolution;
pub mod retrieval;
pub mod scoring;

use crate::config::RememConfig;
use crate::memory::types::{KnowledgeGraphUpdate, MemoryRecord, MemoryResult};
use crate::providers::{EmbeddingProvider, Provider};
use crate::storage::sqlite::SqliteStore;
use crate::storage::vector::VectorIndex;
use crate::storage::MemoryStore;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ReasoningEvent {
    ConsolidationStarted {
        session_id: String,
    },
    FactExtracted {
        content: String,
    },
    ContradictionDetected {
        existing_id: uuid::Uuid,
        new_content: String,
    },
    KnowledgeTripleFound {
        subject: String,
        predicate: String,
        object: String,
    },
    ConsolidationCompleted {
        session_id: String,
        new_facts: usize,
    },
}

#[async_trait::async_trait]
pub trait MemoryHook: Send + Sync {
    async fn before_store(&self, _record: &mut MemoryRecord) -> anyhow::Result<()> {
        Ok(())
    }
    async fn after_store(&self, _record: &MemoryRecord) -> anyhow::Result<()> {
        Ok(())
    }
    async fn before_recall(&self, _query: &mut String) -> anyhow::Result<()> {
        Ok(())
    }
    async fn after_recall(&self, _results: &mut Vec<MemoryResult>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// The reasoning engine orchestrates all intelligent memory operations.
pub struct ReasoningEngine {
    pub config: RememConfig,
    pub provider: Arc<dyn Provider>,
    pub embeddings: Arc<dyn EmbeddingProvider>,
    pub store: Arc<SqliteStore>,
    pub index: Arc<dyn VectorIndex>,
    pub write_counter: AtomicUsize,
    pub event_bus: tokio::sync::broadcast::Sender<ReasoningEvent>,
    pub hooks: Vec<Arc<dyn MemoryHook>>,
    pub mode: tokio::sync::RwLock<crate::config::Mode>,
}

impl ReasoningEngine {
    /// Create a new reasoning engine with the given components.
    pub fn new(
        config: RememConfig,
        provider: Arc<dyn Provider>,
        embeddings: Arc<dyn EmbeddingProvider>,
        store: Arc<SqliteStore>,
        index: Arc<dyn VectorIndex>,
        hooks: Vec<Arc<dyn MemoryHook>>,
    ) -> Self {
        let (event_bus, _) = tokio::sync::broadcast::channel(1024);
        Self {
            config,
            provider,
            embeddings,
            store,
            index,
            write_counter: AtomicUsize::new(0),
            event_bus,
            hooks,
            mode: tokio::sync::RwLock::new(config.memory.mode),
        }
    }

    /// Store a memory with automatic embedding and optional LLM importance scoring.
    pub async fn store_memory(
        &self,
        mut record: MemoryRecord,
        auto_score: bool,
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<MemoryRecord> {
        for hook in &self.hooks {
            hook.before_store(&mut record).await?;
        }

        // Generate embedding
        let embedding = self.embeddings.embed(&record.content, options).await?;
        record.embedding = Some(embedding.clone());

        // Auto-score importance if requested
        if auto_score {
            let importance = scoring::score_importance(
                &*self.provider,
                &record.content,
                &self.config.reasoning.scoring_model,
                options,
            )
            .await?;
            record.importance = importance;
        }

        // Persist to SQLite
        self.store.insert(&record).await?;

        // Add to vector index
        self.index.add(record.id, &embedding).await?;

        tracing::info!(
            id = %record.id,
            importance = record.importance,
            memory_type = %record.memory_type,
            "Stored memory"
        );

        self.check_auto_save().await?;

        for hook in &self.hooks {
            hook.after_store(&record).await?;
        }

        Ok(record)
    }

    /// Guided recall: HNSW search → LLM re-ranking → top-k with reasoning.
    pub async fn recall(
        &self,
        query: &str,
        mut limit: usize,
        filter_tags: &[String],
        since: Option<chrono::DateTime<chrono::Utc>>,
        memory_type: Option<crate::memory::types::MemoryType>,
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<Vec<MemoryResult>> {
        let current_mode = *self.mode.read().await;
        limit = current_mode.adjust_recall_limit(limit);

        let mut query_str = query.to_string();
        for hook in &self.hooks {
            hook.before_recall(&mut query_str).await?;
        }

        let mut results = retrieval::guided_retrieval(
            &*self.provider,
            &*self.embeddings,
            &self.store,
            self.index.as_ref(),
            &query_str,
            limit,
            filter_tags,
            since,
            memory_type,
            &self.config.reasoning.reasoning_model,
            options,
        )
        .await?;

        for hook in &self.hooks {
            hook.after_recall(&mut results).await?;
        }

        Ok(results)
    }

    /// Simple vector + FTS search without LLM re-ranking.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        filter_tags: &[String],
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<Vec<MemoryResult>> {
        // Get embedding for query
        let query_embedding = self.embeddings.embed(query, options).await?;

        // Vector search
        let vector_results = self.index.search(&query_embedding, limit * 2).await?;

        // FTS search
        let fts_results = self.store.search_fts(query, limit).await?;

        // Merge and deduplicate
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();

        // Add vector results first (usually more relevant)
        for vr in &vector_results {
            if seen.insert(vr.id) {
                if let Ok(Some(record)) = self.store.get(vr.id).await {
                    // Apply tag filter
                    if !filter_tags.is_empty()
                        && !filter_tags.iter().any(|t| record.tags.contains(t))
                    {
                        continue;
                    }
                    let mut result = MemoryResult::from(record);
                    result.similarity = vr.similarity;
                    results.push(result);
                }
            }
        }

        // Add FTS results that weren't in vector results
        for record in &fts_results {
            if seen.insert(record.id) {
                if !filter_tags.is_empty() && !filter_tags.iter().any(|t| record.tags.contains(t)) {
                    continue;
                }
                results.push(MemoryResult::from(record.clone()));
            }
        }

        results.truncate(limit);
        Ok(results)
    }

    /// Search the knowledge graph for a specific entity.
    pub async fn get_entity_context(
        &self,
        entity: &str,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        self.store.get_knowledge_for_entity(entity).await
    }

    /// Compact a conversation trace to save context window tokens.
    pub async fn compact_context(
        &self,
        conversation_text: &str,
        focus_areas: Option<&[String]>,
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<compaction::CompactionReport> {
        compaction::compact_context(
            &*self.provider,
            &self.config.reasoning.reasoning_model,
            conversation_text,
            focus_areas,
            options,
        )
        .await
    }

    /// Compress a raw session transcript into durable facts (claude-mem style).
    pub async fn compress_session_transcript(
        &self,
        session_id: &str,
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<crate::memory::types::ConsolidationReport> {
        let _ = self.event_bus.send(ReasoningEvent::ConsolidationStarted {
            session_id: session_id.to_string(),
        });

        // Fetch transcript from SQLite
        let transcript = self.store.get_session_transcript(session_id).await?;
        if transcript.is_empty() {
            return Ok(crate::memory::types::ConsolidationReport {
                session_id: session_id.to_string(),
                new_facts: 0,
                updated_facts: 0,
                contradictions: Vec::new(),
                knowledge_graph_updates: Vec::new(),
            });
        }

        // Format into a single text block
        let mut session_content = String::new();
        for obs in transcript {
            session_content.push_str(&format!(
                "[{}] {}: {}\n",
                obs.timestamp.to_rfc3339(),
                obs.observation_type,
                obs.content
            ));
        }

        // Pass to consolidation engine which extracts facts and resolves contradictions
        let mut facts = consolidation::extract_facts(
            &*self.provider,
            &session_content,
            &self.config.reasoning.reasoning_model,
            options,
        )
        .await?;

        for f in &facts {
            let _ = self.event_bus.send(ReasoningEvent::FactExtracted {
                content: f.content.clone(),
            });
        }

        // Resolve entities in Knowledge Graph triples
        let resolver = resolution::LlmEntityResolver::new(
            &*self.provider,
            self.config.reasoning.reasoning_model.clone(),
            &self.store,
            options,
        );
        use resolution::EntityResolver;

        let mut triples = Vec::new();
        for f in &facts {
            if let Some(t) = &f.knowledge_triple {
                triples.push(t.clone());
            }
        }

        if !triples.is_empty() {
            let resolved_triples = resolver.resolve(triples).await?;
            let mut triple_idx = 0;
            for f in &mut facts {
                if f.knowledge_triple.is_some() {
                    f.knowledge_triple = Some(resolved_triples[triple_idx].clone());
                    triple_idx += 1;
                }
            }
        }

        // Detect contradictions
        let contradictions = contradiction::detect_contradictions(
            &*self.provider,
            &*self.embeddings,
            self.index.as_ref(),
            &*self.store,
            &facts,
            &self.config.reasoning.reasoning_model,
            options,
        )
        .await?;

        // Handle auto-resolution
        for c in &contradictions {
            let _ = self.event_bus.send(ReasoningEvent::ContradictionDetected {
                existing_id: c.existing_memory_id,
                new_content: c.new_content.clone(),
            });
            self.store.archive(c.existing_memory_id).await?;
        }

        // Store new facts
        let mut new_count = 0;
        for fact in facts {
            let mut record =
                MemoryRecord::new(&fact.content, crate::memory::types::MemoryType::Fact);
            record.importance = fact.importance;
            record.tags = fact.tags;
            record.source_session = Some(session_id.to_string());

            // Generate embedding
            let embedding = self.embeddings.embed(&record.content, options).await?;
            record.embedding = Some(embedding.clone());

            self.store.insert(&record).await?;
            self.index.add(record.id, &embedding).await?;

            if let Some(triple) = fact.knowledge_triple {
                let _ = self.event_bus.send(ReasoningEvent::KnowledgeTripleFound {
                    subject: triple.subject.clone(),
                    predicate: triple.predicate.clone(),
                    object: triple.object.clone(),
                });
                self.store
                    .insert_knowledge_triple(&triple, record.id)
                    .await?;
            }
            new_count += 1;
        }

        // Generate and save a session summary
        if let Ok(Some(session)) = self.store.get_session(session_id).await {
            match consolidation::generate_session_summary(
                &*self.provider,
                session_id,
                &session.project,
                &session_content,
                &self.config.reasoning.reasoning_model,
                options,
            )
            .await
            {
                Ok(summary) => {
                    let _ = self.store.insert_session_summary(&summary).await;
                }
                Err(e) => {
                    tracing::warn!("Failed to generate session summary: {}", e);
                }
            }
        }

        let _ = self.event_bus.send(ReasoningEvent::ConsolidationCompleted {
            session_id: session_id.to_string(),
            new_facts: new_count,
        });

        self.check_auto_save().await?;


        Ok(crate::memory::types::ConsolidationReport {
            session_id: session_id.to_string(),
            new_facts: new_count,
            updated_facts: 0,
            contradictions,
            knowledge_graph_updates: Vec::new(),
        })
    }

    /// Query the knowledge graph with filters.
    pub async fn query_knowledge(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        self.store.query_knowledge(subject, predicate, object).await
    }

    /// Update a memory's content, importance, or tags.
    pub async fn update_memory(
        &self,
        id: uuid::Uuid,
        content: Option<String>,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<MemoryRecord> {
        tracing::info!("update_memory: start for id {}", id);
        let mut record = self
            .store
            .get(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Memory not found: {}", id))?;
        tracing::info!("update_memory: fetched from store");

        if let Some(new_content) = content {
            tracing::info!("update_memory: embedding content...");
            record.content = new_content;
            // Re-embed if content changed
            let embedding = self.embeddings.embed(&record.content, options).await?;
            tracing::info!("update_memory: generated embedding");
            record.embedding = Some(embedding.clone());
            tracing::info!("update_memory: adding to index...");
            self.index.add(record.id, &embedding).await?;
            tracing::info!("update_memory: added to index");
        }

        if let Some(new_importance) = importance {
            record.importance = new_importance.clamp(1.0, 10.0);
        }

        if let Some(new_tags) = tags {
            record.tags = new_tags;
        }

        record.updated_at = chrono::Utc::now();
        tracing::info!("update_memory: updating sqlite store...");
        self.store.update(&record).await?;
        tracing::info!("update_memory: updated sqlite store");

        self.check_auto_save().await?;
        tracing::info!("update_memory: finished");

        Ok(record)
    }

    /// Forget a memory (delete, decay, or archive).
    pub async fn forget(
        &self,
        id: uuid::Uuid,
        mode: crate::memory::types::ForgetMode,
    ) -> anyhow::Result<bool> {
        let success = match mode {
            crate::memory::types::ForgetMode::Delete => {
                let _ = self.index.remove(id).await;
                self.store.delete(id).await
            }
            crate::memory::types::ForgetMode::Archive => self.store.archive(id).await,
            crate::memory::types::ForgetMode::Decay => {
                if let Ok(Some(mut record)) = self.store.get(id).await {
                    record.decay_score *= 0.1; // Aggressive decay
                    self.store.update(&record).await?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        };

        if matches!(success, Ok(true)) {
            self.check_auto_save().await?;
        }
        success
    }

    /// Apply decay to all active memories and archive those that fall below the threshold.
    pub async fn apply_decay(&self, decay_factor: f32) -> anyhow::Result<usize> {
        // 1. Update decay scores in the database
        let updated_count = self.store.apply_decay(decay_factor).await?;

        // 2. Get memories that have decayed below the threshold (0.05)
        let to_archive = self.store.get_decayed_ids(0.05).await?;

        let mut archived_count = 0;
        for id in to_archive {
            if self.store.archive(id).await? {
                // Remove from vector index if archived
                let _ = self.index.remove(id).await;
                archived_count += 1;
            }
        }

        tracing::info!(
            updated = updated_count,
            archived = archived_count,
            "Applied memory decay"
        );

        Ok(archived_count)
    }

    // ── TTL Expiration ──────────────────────────────────────────────────

    /// Expire memories that have exceeded their TTL and remove them from the
    /// vector index. Returns the count of newly-archived memories.
    pub async fn expire_ttl(&self) -> anyhow::Result<usize> {
        let expired_ids = self.store.expire_ttl().await?;
        for id in &expired_ids {
            let _ = self.index.remove(*id).await;
        }
        Ok(expired_ids.len())
    }

    // ── Session Management ──────────────────────────────────────────────

    /// Create a new session for the current project.
    pub async fn create_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.store
            .create_session(session_id, &self.config.project)
            .await
    }

    /// End a session.
    pub async fn end_session(&self, session_id: &str) -> anyhow::Result<bool> {
        self.store.end_session(session_id).await
    }

    /// List recent sessions.
    pub async fn list_sessions(
        &self,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::storage::sqlite::SessionRecord>> {
        self.store.list_sessions(limit).await
    }

    /// Get a specific session.
    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<crate::storage::sqlite::SessionRecord>> {
        self.store.get_session(session_id).await
    }

    // ── Memory Stores ───────────────────────────────────────────────────

    /// Create a memory store.
    pub async fn create_store(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<crate::memory::types::MemoryStoreRecord> {
        self.store.create_store(name, description).await
    }

    /// Get a memory store.
    pub async fn get_store(
        &self,
        store_id: &str,
    ) -> anyhow::Result<Option<crate::memory::types::MemoryStoreRecord>> {
        self.store.get_store(store_id).await
    }

    /// List memory stores.
    pub async fn list_stores(
        &self,
    ) -> anyhow::Result<Vec<crate::memory::types::MemoryStoreRecord>> {
        self.store.list_stores().await
    }

    /// Archive a memory store.
    pub async fn archive_store(&self, store_id: &str) -> anyhow::Result<bool> {
        self.store.archive_store(store_id).await
    }

    /// Get memory by path.
    pub async fn get_memory_by_path(
        &self,
        store_id: &str,
        path: &str,
    ) -> anyhow::Result<Option<MemoryRecord>> {
        self.store.get_memory_by_path(store_id, path).await
    }

    /// List memories by store.
    pub async fn list_memories_by_store(
        &self,
        store_id: &str,
    ) -> anyhow::Result<Vec<MemoryRecord>> {
        self.store.list_memories_by_store(store_id).await
    }

    /// List memory versions.
    pub async fn list_memory_versions(
        &self,
        store_id: &str,
        memory_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<crate::memory::types::MemoryVersionRecord>> {
        self.store.list_memory_versions(store_id, memory_id).await
    }

    // ── List Memories ───────────────────────────────────────────────────

    /// List memories with optional filters (delegates to MemoryStore::list).
    pub async fn list_memories(
        &self,
        filter_tags: &[String],
        memory_type: Option<crate::memory::types::MemoryType>,
        since: Option<chrono::DateTime<chrono::Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryRecord>> {
        self.store
            .list(filter_tags, memory_type, since, limit)
            .await
    }

    // ── Index Persistence ───────────────────────────────────────────────

    /// Save the vector index to disk.
    pub async fn save_index(&self) -> anyhow::Result<()> {
        self.index.save(&self.config.index_path()).await
    }

    /// Check if auto-save should run, and trigger it if threshold reached.
    async fn check_auto_save(&self) -> anyhow::Result<()> {
        let count = self.write_counter.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= 100 {
            // Auto-save after 100 writes
            self.write_counter.store(0, Ordering::Relaxed);
            self.save_index().await?;
            tracing::debug!("Auto-saved vector index after 100 writes");
        }
        Ok(())
    }
}

// ── Engine Builder ──────────────────────────────────────────────────────

/// Fluent builder for `ReasoningEngine`.
///
/// ```no_run
/// use rememhq_core::config::RememConfig;
/// use rememhq_core::reasoning::EngineBuilder;
///
/// # async fn demo() -> anyhow::Result<()> {
/// let engine = EngineBuilder::from_config(RememConfig::default())
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct EngineBuilder {
    config: RememConfig,
    provider: Option<Arc<dyn Provider>>,
    embeddings: Option<Arc<dyn EmbeddingProvider>>,
    store: Option<Arc<SqliteStore>>,
    index: Option<Arc<dyn VectorIndex>>,
    max_index_capacity: usize,
    hooks: Vec<Arc<dyn MemoryHook>>,
}

impl EngineBuilder {
    /// Create a builder from a `RememConfig`.
    pub fn from_config(config: RememConfig) -> Self {
        Self {
            config,
            provider: None,
            embeddings: None,
            store: None,
            index: None,
            max_index_capacity: 10_000,
            hooks: Vec::new(),
        }
    }

    /// Override the reasoning provider.
    pub fn with_provider(mut self, p: Arc<dyn Provider>) -> Self {
        self.provider = Some(p);
        self
    }

    /// Override the embedding provider.
    pub fn with_embeddings(mut self, e: Arc<dyn EmbeddingProvider>) -> Self {
        self.embeddings = Some(e);
        self
    }

    /// Override the store.
    pub fn with_store(mut self, s: Arc<SqliteStore>) -> Self {
        self.store = Some(s);
        self
    }

    /// Override the vector index.
    pub fn with_index(mut self, i: Arc<dyn VectorIndex>) -> Self {
        self.index = Some(i);
        self
    }

    /// Set the HNSW max capacity.
    pub fn max_capacity(mut self, n: usize) -> Self {
        self.max_index_capacity = n;
        self
    }

    /// Register a memory hook.
    pub fn with_hook(mut self, hook: Arc<dyn MemoryHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Build the engine, creating default components for anything not overridden.
    pub async fn build(self) -> anyhow::Result<ReasoningEngine> {
        let store = match self.store {
            Some(s) => s,
            None => Arc::new(SqliteStore::open(&self.config.db_path())?),
        };

        let provider = self
            .provider
            .unwrap_or_else(|| crate::providers::factory::build_reasoning_provider(&self.config));

        let embeddings = self
            .embeddings
            .unwrap_or_else(|| crate::providers::factory::build_embedding_provider(&self.config));

        let index = match self.index {
            Some(i) => i,
            None => {
                let i = Arc::new(crate::storage::vector::HNSWVectorIndex::new(
                    embeddings.dimension(),
                    self.max_index_capacity,
                ));
                let _ = i.load(&self.config.index_path()).await;
                i
            }
        };

        Ok(ReasoningEngine::new(
            self.config,
            provider,
            embeddings,
            store,
            index,
            self.hooks,
        ))
    }
}
