//! Provider factory — centralised construction of reasoning and embedding
//! providers with cascading fallbacks.
//!
//! Every binary in the workspace (`cli`, `api`, `mcp`, `ffi`) previously
//! duplicated ~150 lines of provider-init + fallback logic. This module
//! extracts that into two functions so changes are made in one place.

use std::sync::Arc;

use crate::config::RememConfig;
use crate::providers::anthropic::AnthropicProvider;
use crate::providers::embeddings::OpenAIEmbeddings;
use crate::providers::google::{GoogleEmbeddings, GoogleProvider};
use crate::providers::local::{LocalEmbeddings, LocalProvider};
use crate::providers::mock::{MockEmbeddings, MockProvider};
use crate::providers::openai::OpenAIProvider;
use crate::providers::{EmbeddingProvider, Provider};

/// Build a reasoning (LLM completion) provider from configuration.
///
/// Resolution order:
/// 1. `REMEM_REASONING_PROVIDER` env-var override
/// 2. `config.reasoning.provider`
/// 3. Auto-detect from available API keys
///
/// Within each provider, if initialisation fails, cascading fallbacks
/// are tried: configured → alternatives → `MockProvider`.
pub fn build_reasoning_provider(config: &RememConfig) -> Arc<dyn Provider> {
    let name = std::env::var("REMEM_REASONING_PROVIDER")
        .unwrap_or_else(|_| config.reasoning.provider.clone());

    match name.as_str() {
        "openai" => try_provider_chain(&[
            ProviderKind::OpenAI,
            ProviderKind::Anthropic,
            ProviderKind::Google,
        ]),
        "anthropic" => try_provider_chain(&[
            ProviderKind::Anthropic,
            ProviderKind::OpenAI,
            ProviderKind::Google,
        ]),
        "google" | "gemini" => try_provider_chain(&[
            ProviderKind::Google,
            ProviderKind::Anthropic,
            ProviderKind::OpenAI,
        ]),
        "local" => Arc::new(LocalProvider::new(None)),
        "mock" => Arc::new(MockProvider),
        _ => auto_detect_provider(),
    }
}

/// Build an embedding provider from configuration.
///
/// Resolution order mirrors `build_reasoning_provider`, but uses
/// `REMEM_EMBEDDING_PROVIDER` as the override env-var.
pub fn build_embedding_provider(config: &RememConfig) -> Arc<dyn EmbeddingProvider> {
    let name = std::env::var("REMEM_EMBEDDING_PROVIDER")
        .unwrap_or_else(|_| config.reasoning.provider.clone());

    match name.as_str() {
        "google" => match GoogleEmbeddings::new(None) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                tracing::warn!("Failed to initialise Google embeddings: {e}. Trying fallbacks…");
                fallback_embedding()
            }
        },
        "openai" => match OpenAIEmbeddings::new(None, None) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                tracing::warn!("Failed to initialise OpenAI embeddings: {e}. Trying fallbacks…");
                fallback_embedding()
            }
        },
        "mock" => Arc::new(MockEmbeddings::new(768)),
        "local" => try_local_embeddings(),
        _ => auto_detect_embeddings(),
    }
}

// ── Private helpers ─────────────────────────────────────────────────────

enum ProviderKind {
    Anthropic,
    OpenAI,
    Google,
}

fn try_provider(kind: &ProviderKind) -> Option<Arc<dyn Provider>> {
    match kind {
        ProviderKind::Anthropic => AnthropicProvider::new(None).ok().map(|p| Arc::new(p) as _),
        ProviderKind::OpenAI => OpenAIProvider::new(None).ok().map(|p| Arc::new(p) as _),
        ProviderKind::Google => GoogleProvider::new(None).ok().map(|p| Arc::new(p) as _),
    }
}

fn try_provider_chain(chain: &[ProviderKind]) -> Arc<dyn Provider> {
    for (i, kind) in chain.iter().enumerate() {
        match try_provider(kind) {
            Some(p) => return p,
            None if i == 0 => {
                tracing::warn!("Failed to initialise configured provider. Trying fallbacks…");
            }
            None => {}
        }
    }
    tracing::warn!("No valid reasoning API keys found. Falling back to MockProvider.");
    Arc::new(MockProvider)
}

fn auto_detect_provider() -> Arc<dyn Provider> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        if !k.trim().is_empty() {
            if let Some(p) = try_provider(&ProviderKind::Anthropic) {
                return p;
            }
        }
    }
    if let Ok(k) = std::env::var("OPENAI_API_KEY") {
        if !k.trim().is_empty() {
            if let Some(p) = try_provider(&ProviderKind::OpenAI) {
                return p;
            }
        }
    }
    if let Ok(k) = std::env::var("GOOGLE_API_KEY") {
        if !k.trim().is_empty() {
            if let Some(p) = try_provider(&ProviderKind::Google) {
                return p;
            }
        }
    }
    if let Ok(k) = std::env::var("LLAMA_API_BASE") {
        if !k.trim().is_empty() {
            return Arc::new(LocalProvider::new(None));
        }
    }
    if let Ok(k) = std::env::var("OLLAMA_API_BASE") {
        if !k.trim().is_empty() {
            return Arc::new(LocalProvider::new(None));
        }
    }
    tracing::warn!("No reasoning API keys set. Falling back to MockProvider.");
    Arc::new(MockProvider)
}

fn fallback_embedding() -> Arc<dyn EmbeddingProvider> {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        if let Ok(p) = OpenAIEmbeddings::new(None, None) {
            return Arc::new(p);
        }
    }
    if std::env::var("GOOGLE_API_KEY").is_ok() {
        if let Ok(p) = GoogleEmbeddings::new(None) {
            return Arc::new(p);
        }
    }
    tracing::warn!("Falling back to MockEmbeddings.");
    Arc::new(MockEmbeddings::new(768))
}

fn try_local_embeddings() -> Arc<dyn EmbeddingProvider> {
    let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
        .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
    let vocab_path =
        std::env::var("REMEM_LOCAL_VOCAB_PATH").unwrap_or_else(|_| "models/vocab.txt".to_string());
    match LocalEmbeddings::new(&model_path, &vocab_path) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            tracing::warn!(
                "Failed to initialise local embeddings: {e}. Falling back to MockEmbeddings."
            );
            Arc::new(MockEmbeddings::new(768))
        }
    }
}

fn auto_detect_embeddings() -> Arc<dyn EmbeddingProvider> {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        if let Ok(p) = OpenAIEmbeddings::new(None, None) {
            return Arc::new(p);
        }
    }
    if std::env::var("GOOGLE_API_KEY").is_ok() {
        if let Ok(p) = GoogleEmbeddings::new(None) {
            return Arc::new(p);
        }
    }
    // Try local model files
    let model_path = std::env::var("REMEM_LOCAL_MODEL_PATH")
        .unwrap_or_else(|_| "models/nomic-embed-text.onnx".to_string());
    let vocab_path =
        std::env::var("REMEM_LOCAL_VOCAB_PATH").unwrap_or_else(|_| "models/vocab.txt".to_string());
    if std::path::Path::new(&model_path).exists() && std::path::Path::new(&vocab_path).exists() {
        if let Ok(p) = LocalEmbeddings::new(&model_path, &vocab_path) {
            return Arc::new(p);
        }
    }
    tracing::warn!(
        "No embedding API keys or local model files found. Falling back to MockEmbeddings."
    );
    Arc::new(MockEmbeddings::new(768))
}
