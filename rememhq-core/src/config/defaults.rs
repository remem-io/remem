//! Provider-aware model defaults and serde default functions.

use std::path::PathBuf;

/// Return the correct default reasoning model for the active provider.
///
/// Priority: `REMEM_REASONING_MODEL` env var → provider-specific default.
pub fn reasoning_model_for(provider: &str) -> String {
    if let Ok(v) = std::env::var("REMEM_REASONING_MODEL") {
        return v;
    }
    match provider {
        "openai" => "gpt-4o".into(),
        "google" | "gemini" => "gemini-2.5-flash".into(),
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
        "google" | "gemini" => "gemini-2.5-flash".into(),
        "local" => std::env::var("REMEM_LOCAL_MODEL_NAME").unwrap_or_else(|_| "phi-3-mini".into()),
        "mock" => "mock".into(),
        _ => "claude-haiku-4-5".into(), // anthropic default
    }
}

pub fn default_provider() -> String {
    std::env::var("REMEM_PROVIDER").unwrap_or_else(|_| "anthropic".into())
}

pub fn default_reasoning_model() -> String {
    reasoning_model_for(&default_provider())
}

pub fn default_scoring_model() -> String {
    scoring_model_for(&default_provider())
}

pub fn default_working_memory_tokens() -> usize {
    131072
}

pub fn default_decay_interval() -> u32 {
    24
}

pub fn default_data_dir() -> PathBuf {
    std::env::var("REMEM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".remem")
        })
}

pub fn default_hnsw_m() -> usize {
    16
}

pub fn default_hnsw_ef_construction() -> usize {
    200
}

pub fn default_hnsw_ef_search() -> usize {
    100
}

pub fn default_port() -> u16 {
    7474
}

pub fn default_transport() -> String {
    "stdio".into()
}
