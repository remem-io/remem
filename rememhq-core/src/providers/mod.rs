//! Cloud LLM providers for reasoning operations and embedding generation.

pub mod anthropic;
pub mod embeddings;
pub mod factory;
pub mod google;
pub mod local;
pub mod mock;
pub mod openai;
pub mod resiliency;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: ChatMessage,
}

/// Trait for cloud LLM providers used in reasoning operations.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate a completion from the LLM.
    async fn complete(&self, prompt: &str, model: &str) -> anyhow::Result<String>;

    /// Generate a multi-turn chat response, optionally with tool calling.
    async fn chat(&self, messages: &[ChatMessage], tools: &[Tool], model: &str) -> anyhow::Result<ChatResponse>;

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
