//! Core memory types used across the entire remem system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The four memory types in remem's taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    /// Durable facts, decisions, preferences, and patterns.
    Fact,
    /// Structured step sequences for recurring tasks.
    Procedure,
    /// User preferences and settings.
    Preference,
    /// Decisions made and their rationale.
    Decision,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Fact => write!(f, "fact"),
            MemoryType::Procedure => write!(f, "procedure"),
            MemoryType::Preference => write!(f, "preference"),
            MemoryType::Decision => write!(f, "decision"),
        }
    }
}

impl std::str::FromStr for MemoryType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fact" => Ok(MemoryType::Fact),
            "procedure" => Ok(MemoryType::Procedure),
            "preference" => Ok(MemoryType::Preference),
            "decision" => Ok(MemoryType::Decision),
            _ => Err(anyhow::anyhow!("Unknown memory type: {}", s)),
        }
    }
}

/// A single memory record stored in remem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Unique identifier.
    pub id: Uuid,
    /// The actual content of the memory.
    pub content: String,
    /// Embedding vector (populated after embedding).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Importance score (1-10), set by LLM or user.
    pub importance: f32,
    /// Classification tags.
    pub tags: Vec<String>,
    /// Type of memory.
    pub memory_type: MemoryType,
    /// When this memory was created.
    pub created_at: DateTime<Utc>,
    /// When this memory was last updated.
    pub updated_at: DateTime<Utc>,
    /// Decay score — decreases over time, importance-weighted.
    pub decay_score: f32,
    /// Session that produced this memory, if any.
    pub source_session: Option<String>,
    /// Time-to-live in days (None = permanent).
    pub ttl_days: Option<u32>,
}

impl MemoryRecord {
    /// Create a new memory record with sensible defaults.
    pub fn new(content: impl Into<String>, memory_type: MemoryType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            embedding: None,
            importance: 5.0,
            tags: Vec::new(),
            memory_type,
            created_at: now,
            updated_at: now,
            decay_score: 1.0,
            source_session: None,
            ttl_days: None,
        }
    }

    /// Builder-style: set tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Builder-style: set importance.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(1.0, 10.0);
        self
    }

    /// Builder-style: set source session.
    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.source_session = Some(session.into());
        self
    }

    /// Builder-style: set TTL.
    pub fn with_ttl(mut self, days: u32) -> Self {
        self.ttl_days = Some(days);
        self
    }

    /// Builder-style: set embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// A memory result returned from recall/search, includes reasoning trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    /// The memory record.
    pub id: Uuid,
    pub content: String,
    pub importance: f32,
    pub tags: Vec<String>,
    pub memory_type: MemoryType,
    pub created_at: DateTime<Utc>,
    pub source_session: Option<String>,
    /// Relevance score from vector similarity (0.0 - 1.0).
    pub similarity: f32,
    /// Decay score — decreases over time, importance-weighted.
    pub decay_score: f32,
    /// LLM reasoning about why this result is relevant (only in guided recall).
    pub reasoning: Option<String>,
}

impl From<MemoryRecord> for MemoryResult {
    fn from(record: MemoryRecord) -> Self {
        Self {
            id: record.id,
            content: record.content,
            importance: record.importance,
            tags: record.tags,
            memory_type: record.memory_type,
            created_at: record.created_at,
            source_session: record.source_session,
            similarity: 0.0,
            decay_score: record.decay_score,
            reasoning: None,
        }
    }
}

/// Parameters for storing a new memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreRequest {
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// If None, the LLM will score importance automatically.
    pub importance: Option<f32>,
    pub ttl_days: Option<u32>,
    #[serde(default = "default_memory_type")]
    pub memory_type: MemoryType,
}

fn default_memory_type() -> MemoryType {
    MemoryType::Fact
}

/// Parameters for recalling memories (guided retrieval).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallRequest {
    pub query: String,
    #[serde(default = "default_recall_limit")]
    pub limit: usize,
    #[serde(default)]
    pub filter_tags: Vec<String>,
    pub since: Option<DateTime<Utc>>,
    pub memory_type: Option<MemoryType>,
}

fn default_recall_limit() -> usize {
    8
}

/// Parameters for searching memories (no LLM re-ranking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    #[serde(default)]
    pub filter_tags: Vec<String>,
}

fn default_search_limit() -> usize {
    20
}

/// Parameters for updating a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub id: Uuid,
    pub content: Option<String>,
    pub importance: Option<f32>,
    pub tags: Option<Vec<String>>,
}

/// Forget mode for deleting/archiving memories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgetMode {
    Delete,
    Decay,
    Archive,
}

/// Parameters for forgetting a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetRequest {
    pub id: Uuid,
    #[serde(default = "default_forget_mode")]
    pub mode: ForgetMode,
}

fn default_forget_mode() -> ForgetMode {
    ForgetMode::Delete
}

/// Result of a consolidation pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub session_id: String,
    pub new_facts: usize,
    pub updated_facts: usize,
    pub contradictions: Vec<Contradiction>,
    pub knowledge_graph_updates: Vec<KnowledgeGraphUpdate>,
}

/// A detected contradiction between memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub existing_memory_id: Uuid,
    pub new_content: String,
    pub existing_content: String,
    pub explanation: String,
}

/// An update to the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraphUpdate {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}
