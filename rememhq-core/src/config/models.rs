//! Configuration data structures and Mode definitions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::defaults::*;

/// Top-level configuration for a remem instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememConfig {
    pub project: String,
    pub reasoning: ReasoningConfig,
    pub memory: MemoryConfig,
    pub storage: StorageConfig,
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    /// Cloud provider: "anthropic", "openai", "google", "local", "mock"
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Model for consolidation + guided retrieval.
    /// Defaults are provider-aware: Anthropic → claude-sonnet-4-5,
    /// OpenAI → gpt-4o, Google → gemini-2.0-flash.
    #[serde(default = "default_reasoning_model")]
    pub reasoning_model: String,
    /// Model for importance scoring + contradiction pre-check.
    /// Defaults are provider-aware: Anthropic → claude-haiku-4-5,
    /// OpenAI → gpt-4o-mini, Google → gemini-2.0-flash.
    #[serde(default = "default_scoring_model")]
    pub scoring_model: String,
    /// Path to local GGUF model (only for provider = "local")
    pub local_model_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Max tokens for working memory
    #[serde(default = "default_working_memory_tokens")]
    pub working_memory_tokens: usize,
    /// Hours between importance decay passes
    #[serde(default = "default_decay_interval")]
    pub importance_decay_interval_hours: u32,
    /// Whether to keep raw session logs after consolidation
    #[serde(default)]
    pub keep_raw_sessions: bool,
    /// Directory to watch for transcript files
    pub transcript_watch_dir: Option<PathBuf>,
    /// The current memory mode
    #[serde(default)]
    pub mode: Mode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Root data directory
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// HNSW M parameter (connections per node)
    #[serde(default = "default_hnsw_m")]
    pub hnsw_m: usize,
    /// HNSW ef_construction parameter
    #[serde(default = "default_hnsw_ef_construction")]
    pub hnsw_ef_construction: usize,
    /// HNSW ef_search parameter
    #[serde(default = "default_hnsw_ef_search")]
    pub hnsw_ef_search: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// REST API port
    #[serde(default = "default_port")]
    pub port: u16,
    /// MCP transport: "stdio", "http-sse", "http-polling"
    #[serde(default = "default_transport")]
    pub transport: String,
}

/// Agent memory mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Mode {
    #[default]
    Standard,
    Debugging,
    Refactoring,
    Exploration,
    Writing,
}

impl Mode {
    pub fn adjust_recall_limit(&self, limit: usize) -> usize {
        match self {
            Self::Debugging => limit * 2,
            Self::Writing => std::cmp::max(1, limit / 2),
            _ => limit,
        }
    }

    pub fn adjust_token_budget(&self, budget: usize) -> usize {
        match self {
            Self::Exploration => budget + 2000,
            Self::Refactoring => budget + 4000,
            _ => budget,
        }
    }
}
