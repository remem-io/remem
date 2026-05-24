//! # rememhq — Rust SDK for remem
//!
//! A clean, consumer-facing Rust SDK for the remem reasoning memory layer.
//! Provides a high-level `Memory` builder API that wraps `rememhq-core`
//! and handles provider initialization, configuration, and storage setup.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use rememhq::{Memory, MemoryType};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mem = Memory::builder()
//!         .project("my-agent")
//!         .build()
//!         .await?;
//!
//!     // Store a memory
//!     let record = mem.store("rate limiting uses a token bucket at 1000 req/min", &["api"], None).await?;
//!     println!("Stored: {} (importance: {:.1})", record.id, record.importance);
//!
//!     // Recall with LLM re-ranking
//!     let results = mem.recall("api rate limits", 5).await?;
//!     for r in &results {
//!         println!("{} (importance: {})", r.content, r.importance);
//!     }
//!
//!     Ok(())
//! }
//! ```

use std::sync::Arc;

// Re-export core types for consumer convenience
pub use rememhq_core::config::RememConfig;
pub use rememhq_core::memory::types::{
    ConsolidationReport, ForgetMode, KnowledgeGraphUpdate, MemoryRecord, MemoryResult, MemoryType,
    StoreRequest,
};
pub use rememhq_core::providers::{EmbeddingProvider, Provider};
pub use rememhq_core::reasoning::ReasoningEngine;
pub use rememhq_core::storage::vector::VectorIndex;
pub use rememhq_core::storage::MemoryStore;

/// Available reasoning model presets.
#[derive(Debug, Clone)]
pub enum ReasoningModel {
    /// Anthropic Claude Sonnet (default)
    ClaudeSonnet,
    /// Anthropic Claude Haiku (fast scoring)
    ClaudeHaiku,
    /// OpenAI GPT-4o
    Gpt4o,
    /// OpenAI GPT-4o-mini
    Gpt4oMini,
    /// Google Gemini 2.0 Flash
    Gemini2Flash,
    /// Custom model name
    Custom(String),
}

impl ReasoningModel {
    /// Get the provider name for this model.
    pub fn provider(&self) -> &str {
        match self {
            Self::ClaudeSonnet | Self::ClaudeHaiku => "anthropic",
            Self::Gpt4o | Self::Gpt4oMini => "openai",
            Self::Gemini2Flash => "google",
            Self::Custom(_) => "anthropic",
        }
    }

    /// Get the model identifier string.
    pub fn model_name(&self) -> &str {
        match self {
            Self::ClaudeSonnet => "claude-sonnet-4-5",
            Self::ClaudeHaiku => "claude-haiku-4-5",
            Self::Gpt4o => "gpt-4o",
            Self::Gpt4oMini => "gpt-4o-mini",
            Self::Gemini2Flash => "gemini-2.0-flash",
            Self::Custom(name) => name,
        }
    }
}

/// Builder for constructing a [`Memory`] instance.
///
/// ```rust,no_run
/// use rememhq::{Memory, ReasoningModel};
///
/// # async fn example() -> anyhow::Result<()> {
/// let mem = Memory::builder()
///     .project("my-agent")
///     .reasoning_model(ReasoningModel::ClaudeSonnet)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct MemoryBuilder {
    project: String,
    reasoning_model: Option<ReasoningModel>,
    data_dir: Option<String>,
    vector_dimensions: usize,
    max_vectors: usize,
}

impl Default for MemoryBuilder {
    fn default() -> Self {
        Self {
            project: "default".into(),
            reasoning_model: None,
            data_dir: None,
            vector_dimensions: 768,
            max_vectors: 10_000,
        }
    }
}

impl MemoryBuilder {
    /// Set the project name for memory isolation.
    pub fn project(mut self, project: &str) -> Self {
        self.project = project.to_string();
        self
    }

    /// Set the reasoning model to use.
    pub fn reasoning_model(mut self, model: ReasoningModel) -> Self {
        self.reasoning_model = Some(model);
        self
    }

    /// Override the data directory (defaults to `~/.remem`).
    pub fn data_dir(mut self, dir: &str) -> Self {
        self.data_dir = Some(dir.to_string());
        self
    }

    /// Set vector index dimensions (defaults to 768).
    pub fn vector_dimensions(mut self, dims: usize) -> Self {
        self.vector_dimensions = dims;
        self
    }

    /// Set maximum vector capacity (defaults to 10,000).
    pub fn max_vectors(mut self, max: usize) -> Self {
        self.max_vectors = max;
        self
    }

    /// Build and initialize the `Memory` instance.
    ///
    /// This will:
    /// 1. Load or create the configuration file.
    /// 2. Open (or create) the SQLite database.
    /// 3. Initialize the HNSW vector index.
    /// 4. Auto-detect and initialize the best available provider.
    pub async fn build(self) -> anyhow::Result<Memory> {
        use rememhq_core::providers::anthropic::AnthropicProvider;
        use rememhq_core::providers::embeddings::OpenAIEmbeddings;
        use rememhq_core::providers::google::{GoogleEmbeddings, GoogleProvider};
        use rememhq_core::providers::openai::OpenAIProvider;
        use rememhq_core::storage::sqlite::SqliteStore;
        use rememhq_core::storage::vector::HNSWVectorIndex;

        let data_dir_path = self.data_dir.as_ref().map(std::path::Path::new);
        let config = RememConfig::load(&self.project, data_dir_path)?;

        let store = Arc::new(SqliteStore::open(&config.db_path())?);
        let index = Arc::new(HNSWVectorIndex::new(
            self.vector_dimensions,
            self.max_vectors,
        ));
        let _ = index.load(&config.index_path()).await;

        // Determine which provider to use
        let target_provider = self
            .reasoning_model
            .as_ref()
            .map(|m| m.provider().to_string())
            .unwrap_or_else(|| config.reasoning.provider.clone());

        // Initialize reasoning provider with cascading fallback
        let provider: Arc<dyn Provider> = match target_provider.as_str() {
            "openai" => match OpenAIProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(_) => match AnthropicProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => match GoogleProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                    },
                },
            },
            "google" => match GoogleProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(_) => match AnthropicProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => match OpenAIProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                    },
                },
            },
            "mock" => Arc::new(rememhq_core::providers::mock::MockProvider),
            _ => match AnthropicProvider::new(None) {
                Ok(p) => Arc::new(p),
                Err(_) => match OpenAIProvider::new(None) {
                    Ok(p) => Arc::new(p),
                    Err(_) => match GoogleProvider::new(None) {
                        Ok(p) => Arc::new(p),
                        Err(_) => Arc::new(rememhq_core::providers::mock::MockProvider),
                    },
                },
            },
        };

        // Initialize embedding provider with cascading fallback
        let embeddings: Arc<dyn EmbeddingProvider> = if std::env::var("OPENAI_API_KEY").is_ok() {
            match OpenAIEmbeddings::new(None, Some(self.vector_dimensions)) {
                Ok(p) => Arc::new(p),
                Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(
                    self.vector_dimensions,
                )),
            }
        } else if std::env::var("GOOGLE_API_KEY").is_ok() {
            match GoogleEmbeddings::new(None) {
                Ok(p) => Arc::new(p),
                Err(_) => Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(
                    self.vector_dimensions,
                )),
            }
        } else {
            Arc::new(rememhq_core::providers::mock::MockEmbeddings::new(
                self.vector_dimensions,
            ))
        };

        let engine = ReasoningEngine::new(config.clone(), provider, embeddings, store, index);

        Ok(Memory {
            engine: Arc::new(engine),
            config,
        })
    }
}

/// High-level memory client for the remem reasoning memory layer.
///
/// Create with [`Memory::builder()`].
pub struct Memory {
    engine: Arc<ReasoningEngine>,
    config: RememConfig,
}

impl Memory {
    /// Create a new `MemoryBuilder`.
    pub fn builder() -> MemoryBuilder {
        MemoryBuilder::default()
    }

    /// Store a memory with optional tags and importance.
    ///
    /// If `importance` is `None`, the LLM will auto-score it.
    pub async fn store(
        &self,
        content: &str,
        tags: &[&str],
        importance: Option<f32>,
    ) -> anyhow::Result<MemoryRecord> {
        let tag_list: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        let auto_score = importance.is_none();
        let mut record = MemoryRecord::new(content, MemoryType::Fact).with_tags(tag_list);
        if let Some(imp) = importance {
            record = record.with_importance(imp);
        }
        self.engine.store_memory(record, auto_score).await
    }

    /// Store a typed memory (Fact, Procedure, Preference, Episode).
    pub async fn store_typed(
        &self,
        content: &str,
        memory_type: MemoryType,
        tags: &[&str],
        importance: Option<f32>,
    ) -> anyhow::Result<MemoryRecord> {
        let tag_list: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        let auto_score = importance.is_none();
        let mut record = MemoryRecord::new(content, memory_type).with_tags(tag_list);
        if let Some(imp) = importance {
            record = record.with_importance(imp);
        }
        self.engine.store_memory(record, auto_score).await
    }

    /// Recall memories using LLM-guided retrieval (semantic + re-ranking).
    pub async fn recall(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryResult>> {
        self.engine.recall(query, limit, &[], None, None).await
    }

    /// Recall with tag filtering.
    pub async fn recall_with_tags(
        &self,
        query: &str,
        limit: usize,
        tags: &[String],
    ) -> anyhow::Result<Vec<MemoryResult>> {
        self.engine.recall(query, limit, tags, None, None).await
    }

    /// Simple vector + FTS search without LLM re-ranking.
    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryResult>> {
        self.engine.search(query, limit, &[]).await
    }

    /// Update a memory's content, importance, or tags.
    pub async fn update(
        &self,
        id: uuid::Uuid,
        content: Option<String>,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
    ) -> anyhow::Result<MemoryRecord> {
        self.engine
            .update_memory(id, content, importance, tags)
            .await
    }

    /// Forget a memory by ID.
    pub async fn forget(&self, id: uuid::Uuid, mode: ForgetMode) -> anyhow::Result<bool> {
        self.engine.forget(id, mode).await
    }

    /// Apply importance-weighted decay to all active memories.
    pub async fn decay(&self, factor: f32) -> anyhow::Result<usize> {
        self.engine.apply_decay(factor).await
    }

    /// Query the knowledge graph.
    pub async fn query_knowledge(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        self.engine
            .query_knowledge(subject, predicate, object)
            .await
    }

    /// Get entity context from the knowledge graph.
    pub async fn get_entity_context(
        &self,
        entity: &str,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        self.engine.get_entity_context(entity).await
    }

    /// Save the vector index to disk.
    pub async fn save_index(&self) -> anyhow::Result<()> {
        self.engine.index.save(&self.config.index_path()).await
    }

    /// Get direct access to the underlying `ReasoningEngine`.
    pub fn engine(&self) -> &Arc<ReasoningEngine> {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_model_provider() {
        assert_eq!(ReasoningModel::ClaudeSonnet.provider(), "anthropic");
        assert_eq!(ReasoningModel::ClaudeHaiku.provider(), "anthropic");
        assert_eq!(ReasoningModel::Gpt4o.provider(), "openai");
        assert_eq!(ReasoningModel::Gpt4oMini.provider(), "openai");
        assert_eq!(ReasoningModel::Gemini2Flash.provider(), "google");
        assert_eq!(
            ReasoningModel::Custom("my-model".into()).provider(),
            "anthropic"
        );
    }

    #[test]
    fn test_reasoning_model_name() {
        assert_eq!(
            ReasoningModel::ClaudeSonnet.model_name(),
            "claude-sonnet-4-5"
        );
        assert_eq!(ReasoningModel::ClaudeHaiku.model_name(), "claude-haiku-4-5");
        assert_eq!(ReasoningModel::Gpt4o.model_name(), "gpt-4o");
        assert_eq!(ReasoningModel::Gpt4oMini.model_name(), "gpt-4o-mini");
        assert_eq!(
            ReasoningModel::Gemini2Flash.model_name(),
            "gemini-2.0-flash"
        );
        assert_eq!(
            ReasoningModel::Custom("my-model".into()).model_name(),
            "my-model"
        );
    }

    #[test]
    fn test_builder_defaults() {
        let builder = MemoryBuilder::default();
        assert_eq!(builder.project, "default");
        assert!(builder.reasoning_model.is_none());
        assert!(builder.data_dir.is_none());
        assert_eq!(builder.vector_dimensions, 768);
        assert_eq!(builder.max_vectors, 10_000);
    }

    #[test]
    fn test_builder_configuration() {
        let builder = Memory::builder()
            .project("test-project")
            .reasoning_model(ReasoningModel::Gpt4o)
            .data_dir("/tmp/test")
            .vector_dimensions(512)
            .max_vectors(5000);

        assert_eq!(builder.project, "test-project");
        assert!(builder.reasoning_model.is_some());
        assert_eq!(builder.data_dir, Some("/tmp/test".into()));
        assert_eq!(builder.vector_dimensions, 512);
        assert_eq!(builder.max_vectors, 5000);
    }

    #[tokio::test]
    async fn test_memory_build_with_mock() {
        // This should always succeed since it falls back to MockProvider
        let mem = Memory::builder()
            .project(&format!("sdk-test-{}", uuid::Uuid::new_v4().as_simple()))
            .build()
            .await;

        assert!(mem.is_ok(), "Memory::builder().build() should not fail");
    }
}
