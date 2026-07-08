//! Core memory types used across the entire remem system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The four memory types in remem's taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    /// An observation from a session trace.
    Observation,
}

/// Detailed classification of an observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    CodePattern,
    FileOperation,
    ErrorResolution,
    EnvironmentConfig,
    DependencyInfo,
    UserPreference,
    General,
}

impl std::fmt::Display for ObservationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObservationKind::CodePattern => write!(f, "code_pattern"),
            ObservationKind::FileOperation => write!(f, "file_operation"),
            ObservationKind::ErrorResolution => write!(f, "error_resolution"),
            ObservationKind::EnvironmentConfig => write!(f, "environment_config"),
            ObservationKind::DependencyInfo => write!(f, "dependency_info"),
            ObservationKind::UserPreference => write!(f, "user_preference"),
            ObservationKind::General => write!(f, "general"),
        }
    }
}

impl std::str::FromStr for ObservationKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "code_pattern" => Ok(ObservationKind::CodePattern),
            "file_operation" => Ok(ObservationKind::FileOperation),
            "error_resolution" => Ok(ObservationKind::ErrorResolution),
            "environment_config" => Ok(ObservationKind::EnvironmentConfig),
            "dependency_info" => Ok(ObservationKind::DependencyInfo),
            "user_preference" => Ok(ObservationKind::UserPreference),
            "general" => Ok(ObservationKind::General),
            _ => Err(anyhow::anyhow!("Unknown observation kind: {}", s)),
        }
    }
}

/// An observation made during an agent session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionObservation {
    /// Unique ID for this observation
    pub id: Uuid,
    /// Optional parent observation ID to support session branching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    /// The session this observation belongs to
    pub session_id: String,
    /// Type of observation (e.g., "tool_call", "prompt", "result")
    pub observation_type: String,
    /// The actual content/payload
    pub content: String,
    /// When it was recorded
    pub timestamp: DateTime<Utc>,
}

impl SessionObservation {
    pub fn new(
        session_id: impl Into<String>,
        observation_type: impl Into<String>,
        content: impl Into<String>,
        parent_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id,
            session_id: session_id.into(),
            observation_type: observation_type.into(),
            content: content.into(),
            timestamp: Utc::now(),
        }
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Fact => write!(f, "fact"),
            MemoryType::Procedure => write!(f, "procedure"),
            MemoryType::Preference => write!(f, "preference"),
            MemoryType::Decision => write!(f, "decision"),
            MemoryType::Observation => write!(f, "observation"),
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
            "observation" => Ok(MemoryType::Observation),
            _ => Err(anyhow::anyhow!("Unknown memory type: {}", s)),
        }
    }
}

/// A single memory record stored in remem.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    /// The fine-grained observation kind, if this is an observation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_kind: Option<ObservationKind>,
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
    /// The memory store this memory belongs to (None = legacy global store).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_id: Option<String>,
    /// The file-like path for the memory within the store (e.g., "preferences.txt").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
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
            observation_kind: None,
            created_at: now,
            updated_at: now,
            decay_score: 1.0,
            source_session: None,
            ttl_days: None,
            store_id: None,
            path: None,
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

    /// Builder-style: set observation kind.
    pub fn with_observation_kind(mut self, kind: ObservationKind) -> Self {
        self.observation_kind = Some(kind);
        self
    }
}

/// A memory result returned from recall/search, includes reasoning trace.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

/// A workspace-scoped memory store.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MemoryStoreRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

/// An immutable version record of a memory's history.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MemoryVersionRecord {
    pub id: String,
    pub store_id: String,
    pub memory_id: Uuid,
    pub operation: String,
    pub content: String,
    pub content_sha256: String,
    pub created_at: DateTime<Utc>,
}

/// A summary of a session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionSummaryRecord {
    pub session_id: String,
    pub project: String,
    pub summary: String,
    pub files_touched: Vec<String>,
    pub key_decisions: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

/// Parameters for storing a new memory.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateRequest {
    pub id: Uuid,
    pub content: Option<String>,
    pub importance: Option<f32>,
    pub tags: Option<Vec<String>>,
}

/// Forget mode for deleting/archiving memories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ForgetMode {
    Delete,
    Decay,
    Archive,
}

/// Parameters for forgetting a memory.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForgetRequest {
    pub id: Uuid,
    #[serde(default = "default_forget_mode")]
    pub mode: ForgetMode,
}

fn default_forget_mode() -> ForgetMode {
    ForgetMode::Delete
}

/// Result of a consolidation pass.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConsolidationReport {
    pub session_id: String,
    pub new_facts: usize,
    pub updated_facts: usize,
    pub contradictions: Vec<Contradiction>,
    pub knowledge_graph_updates: Vec<KnowledgeGraphUpdate>,
}

/// A detected contradiction between memories.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Contradiction {
    pub existing_memory_id: Uuid,
    pub new_content: String,
    pub existing_content: String,
    pub explanation: String,
}

/// An update to the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KnowledgeGraphUpdate {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}
