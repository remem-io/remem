//! Cloud LLM providers for reasoning operations and embedding generation.

pub mod anthropic;
pub mod embeddings;
pub mod google;
pub mod local;
pub mod mock;
pub mod openai;

use async_trait::async_trait;

/// Trait for cloud LLM providers used in reasoning operations.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate a completion from the LLM.
    async fn complete(&self, prompt: &str, model: &str) -> anyhow::Result<String>;

    /// Get the provider name.
    fn name(&self) -> &str;
}

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch).
    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;

    /// Embedding dimension.
    fn dimension(&self) -> usize;
}
