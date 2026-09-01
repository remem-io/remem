//! Configuration for remem.
//!
//! Reads from `.remem/config.toml` in the project directory, falling back
//! to environment variables for all settings.

pub mod defaults;
pub mod models;

use std::path::PathBuf;

pub use defaults::{reasoning_model_for, scoring_model_for};
pub use models::{MemoryConfig, Mode, ReasoningConfig, RememConfig, ServerConfig, StorageConfig};

use defaults::*;

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
                transcript_watch_dir: None,
                mode: Mode::Standard,
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
    /// Merge project-local configuration, rejecting any denylisted keys.
    ///
    /// Project-local configs from `.remem/config.toml` in a repository are
    /// potentially untrusted (any contributor can modify them). Sensitive
    /// fields like provider selection and data directories are blocked to
    /// prevent exfiltration attacks.
    pub fn merge_project_config(&mut self, project_config: &RememConfig) {
        // Only merge safe fields from project config
        // memory.* fields are safe
        self.memory.working_memory_tokens = project_config.memory.working_memory_tokens;
        self.memory.importance_decay_interval_hours =
            project_config.memory.importance_decay_interval_hours;
        self.memory.keep_raw_sessions = project_config.memory.keep_raw_sessions;
        self.memory.transcript_watch_dir = project_config.memory.transcript_watch_dir.clone();
        self.memory.mode = project_config.memory.mode;

        // server port and transport are safe
        self.server.port = project_config.server.port;
        self.server.transport = project_config.server.transport.clone();

        // storage.hnsw_* tuning params are safe
        self.storage.hnsw_m = project_config.storage.hnsw_m;
        self.storage.hnsw_ef_construction = project_config.storage.hnsw_ef_construction;
        self.storage.hnsw_ef_search = project_config.storage.hnsw_ef_search;

        // BLOCKED: reasoning.provider, reasoning.local_model_path, storage.data_dir
        // These are security-sensitive and only settable via user config or env vars
        tracing::debug!("Project-local config applied (denylisted keys skipped)");
    }

    /// Load config from `.remem/config.toml` in the given project directory,
    /// falling back to defaults and environment variables.
    pub fn load(project: &str, project_dir: Option<&std::path::Path>) -> anyhow::Result<Self> {
        // Start with defaults (includes env var overrides via serde defaults)
        let mut config = RememConfig::default();

        // Layer 1: User-level config (~/.remem/config.toml) — fully trusted
        let user_config_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".remem")
            .join("config.toml");
        if user_config_path.exists() {
            let raw = std::fs::read_to_string(&user_config_path)?;
            config = toml::from_str::<RememConfig>(&raw)?;
        }

        // Layer 2: Project-local config — restricted by denylist
        if let Some(dir) = project_dir {
            let project_config_path = dir.join(".remem").join("config.toml");
            if project_config_path.exists() {
                let raw = std::fs::read_to_string(&project_config_path)?;
                let project_config = toml::from_str::<RememConfig>(&raw)?;
                config.merge_project_config(&project_config);
            }
        }

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

    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

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
    fn test_mode_adjust_recall_limit() {
        let limit = 10;
        assert_eq!(Mode::Standard.adjust_recall_limit(limit), 10);
        assert_eq!(Mode::Debugging.adjust_recall_limit(limit), 20);
        assert_eq!(Mode::Writing.adjust_recall_limit(limit), 5);
        assert_eq!(Mode::Writing.adjust_recall_limit(1), 1);
        assert_eq!(Mode::Exploration.adjust_recall_limit(limit), 10);
        assert_eq!(Mode::Refactoring.adjust_recall_limit(limit), 10);
    }

    #[test]
    fn test_mode_adjust_token_budget() {
        let budget = 4000;
        assert_eq!(Mode::Standard.adjust_token_budget(budget), 4000);
        assert_eq!(Mode::Exploration.adjust_token_budget(budget), 6000);
        assert_eq!(Mode::Refactoring.adjust_token_budget(budget), 8000);
        assert_eq!(Mode::Debugging.adjust_token_budget(budget), 4000);
        assert_eq!(Mode::Writing.adjust_token_budget(budget), 4000);
    }

    #[test]
    fn test_reasoning_model_for_google() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(reasoning_model_for("google"), "gemini-2.5-flash");
    }

    #[test]
    fn test_scoring_model_for_google() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(scoring_model_for("google"), "gemini-2.5-flash");
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
        assert_eq!(config.reasoning.provider, "anthropic");
        assert_eq!(config.reasoning.reasoning_model, "claude-sonnet-4-5");
        assert_eq!(config.reasoning.scoring_model, "claude-haiku-4-5");
    }

    #[test]
    fn test_project_config_denylist_blocks_provider() {
        let mut base = RememConfig::default();
        let mut project = RememConfig::default();
        project.reasoning.provider = "evil-provider".into();
        project.storage.data_dir = PathBuf::from("/tmp/evil");
        project.memory.working_memory_tokens = 999;

        base.merge_project_config(&project);

        assert_eq!(base.reasoning.provider, "anthropic");
        assert_ne!(base.storage.data_dir, PathBuf::from("/tmp/evil"));
        assert_eq!(base.memory.working_memory_tokens, 999);
    }
}
