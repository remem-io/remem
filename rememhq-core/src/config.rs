//! Configuration for remem.
//!
//! Reads from `.remem/config.toml` in the project directory, falling back
//! to environment variables for all settings.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

// ---------------------------------------------------------------------------
// Provider-aware model defaults
// ---------------------------------------------------------------------------

/// Return the correct default reasoning model for the active provider.
///
/// Priority: `REMEM_REASONING_MODEL` env var → provider-specific default.
pub fn reasoning_model_for(provider: &str) -> String {
    if let Ok(v) = std::env::var("REMEM_REASONING_MODEL") {
        return v;
    }
    match provider {
        "openai" => "gpt-4o".into(),
        "google" => "gemini-2.0-flash".into(),
        "local" => std::env::var("REMEM_LOCAL_MODEL_NAME").unwrap_or_else(|_| "phi-3-mini".into()),
        "mock" => "mock".into(),
        _ => "claude-sonnet-4-5".into(), // anthropic default
    }
}

/// Return the correct default scoring model for the active provider.
///
/// Priority: `REMEM_SCORING_MODEL` env var → provider-specific default.
pub fn scoring_model_for(provider: &str) -> String {
    if let Ok(v) = std::env::var("REMEM_SCORING_MODEL") {
        return v;
    }
    match provider {
        "openai" => "gpt-4o-mini".into(),
        "google" => "gemini-2.0-flash".into(),
        "local" => std::env::var("REMEM_LOCAL_MODEL_NAME").unwrap_or_else(|_| "phi-3-mini".into()),
        "mock" => "mock".into(),
        _ => "claude-haiku-4-5".into(), // anthropic default
    }
}

// ---------------------------------------------------------------------------
// Serde defaults (used when deserialising config.toml without explicit values)
// ---------------------------------------------------------------------------

fn default_provider() -> String {
    std::env::var("REMEM_PROVIDER").unwrap_or_else(|_| "anthropic".into())
}

fn default_reasoning_model() -> String {
    reasoning_model_for(&default_provider())
}

fn default_scoring_model() -> String {
    scoring_model_for(&default_provider())
}

fn default_working_memory_tokens() -> usize {
    131072
}
fn default_decay_interval() -> u32 {
    24
}
fn default_data_dir() -> PathBuf {
    std::env::var("REMEM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".remem")
        })
}
fn default_hnsw_m() -> usize {
    16
}
fn default_hnsw_ef_construction() -> usize {
    200
}
fn default_hnsw_ef_search() -> usize {
    100
}
fn default_port() -> u16 {
    7474
}
fn default_transport() -> String {
    "stdio".into()
}

impl Default for RememConfig {
    fn default() -> Self {
        let provider = default_provider();
        Self {
            project: "default".into(),
            reasoning: ReasoningConfig {
                reasoning_model: reasoning_model_for(&provider),
                scoring_model: scoring_model_for(&provider),
                local_model_path: None,
                provider,
            },
            memory: MemoryConfig {
                working_memory_tokens: default_working_memory_tokens(),
                importance_decay_interval_hours: default_decay_interval(),
                keep_raw_sessions: false,
            },
            storage: StorageConfig {
                data_dir: default_data_dir(),
                hnsw_m: default_hnsw_m(),
                hnsw_ef_construction: default_hnsw_ef_construction(),
                hnsw_ef_search: default_hnsw_ef_search(),
            },
            server: ServerConfig {
                port: default_port(),
                transport: default_transport(),
            },
        }
    }
}

impl RememConfig {
    /// Load config from `.remem/config.toml` in the given project directory,
    /// falling back to defaults and environment variables.
    pub fn load(project: &str, project_dir: Option<&std::path::Path>) -> anyhow::Result<Self> {
        let mut config = if let Some(dir) = project_dir {
            let config_path = dir.join(".remem").join("config.toml");
            if config_path.exists() {
                let raw = std::fs::read_to_string(&config_path)?;
                toml::from_str::<RememConfig>(&raw)?
            } else {
                RememConfig::default()
            }
        } else {
            RememConfig::default()
        };

        config.project = project.to_string();
        Ok(config)
    }

    /// Returns the project-specific data directory.
    pub fn project_data_dir(&self) -> PathBuf {
        self.storage.data_dir.join("projects").join(&self.project)
    }

    /// Returns the path where the SQLite database should be stored.
    pub fn db_path(&self) -> PathBuf {
        self.project_data_dir().join("remem.db")
    }

    /// Returns the path where the HNSW index should be stored.
    pub fn index_path(&self) -> PathBuf {
        self.project_data_dir().join("hnsw.idx")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `std::env::set_var`/`remove_var` mutate process-wide global state, and
    // `cargo test` runs tests in parallel by default. Without serialization,
    // these tests race against each other (and against any other test that
    // reads REMEM_PROVIDER / REMEM_REASONING_MODEL / REMEM_SCORING_MODEL),
    // causing intermittent, platform-dependent failures. This mutex ensures
    // only one of these tests touches the environment at a time.
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Clears all env vars these tests depend on. Call while holding the lock.
    fn clear_env() {
        std::env::remove_var("REMEM_PROVIDER");
        std::env::remove_var("REMEM_REASONING_MODEL");
        std::env::remove_var("REMEM_SCORING_MODEL");
    }

    #[test]
    fn test_reasoning_model_for_anthropic() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(reasoning_model_for("anthropic"), "claude-sonnet-4-5");
    }

    #[test]
    fn test_reasoning_model_for_openai() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(reasoning_model_for("openai"), "gpt-4o");
    }

    #[test]
    fn test_reasoning_model_for_google() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(reasoning_model_for("google"), "gemini-2.0-flash");
    }

    #[test]
    fn test_scoring_model_for_google() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(scoring_model_for("google"), "gemini-2.0-flash");
    }

    #[test]
    fn test_scoring_model_for_openai() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(scoring_model_for("openai"), "gpt-4o-mini");
    }

    #[test]
    fn test_reasoning_model_env_override() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("REMEM_REASONING_MODEL", "my-custom-model");
        let result = reasoning_model_for("google");
        clear_env();
        assert_eq!(result, "my-custom-model");
    }

    #[test]
    fn test_default_config_provider_aware_models() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        let config = RememConfig::default();
        // Default provider is anthropic
        assert_eq!(config.reasoning.provider, "anthropic");
        assert_eq!(config.reasoning.reasoning_model, "claude-sonnet-4-5");
        assert_eq!(config.reasoning.scoring_model, "claude-haiku-4-5");
    }
}
