use crate::providers::{EmbeddingProvider, Provider};
use crate::storage::vector::remem_ffi;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub struct LocalEmbeddings {
    handle: *mut remem_ffi::remem_embedder_t,
    dim: usize,
}

// SAFETY: The C++ embedder is thread-safe if implemented correctly (stateless inference)
unsafe impl Send for LocalEmbeddings {}
unsafe impl Sync for LocalEmbeddings {}

impl LocalEmbeddings {
    pub fn new(model_path: &str, vocab_path: &str) -> anyhow::Result<Self> {
        let c_model_path = std::ffi::CString::new(model_path)?;
        let c_vocab_path = std::ffi::CString::new(vocab_path)?;
        let handle =
            unsafe { remem_ffi::remem_embedder_new(c_model_path.as_ptr(), c_vocab_path.as_ptr()) };

        if handle.is_null() {
            return Err(anyhow::anyhow!("Failed to initialize local embedder"));
        }

        let dim = unsafe { remem_ffi::remem_embedder_dim(handle) };

        Ok(Self { handle, dim })
    }
}

impl Drop for LocalEmbeddings {
    fn drop(&mut self) {
        unsafe {
            remem_ffi::remem_embedder_free(self.handle);
        }
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddings {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let c_text = std::ffi::CString::new(text)?;
        let mut out_dim = 0;

        let ptr =
            unsafe { remem_ffi::remem_embed_text(self.handle, c_text.as_ptr(), &mut out_dim) };

        if ptr.is_null() {
            return Err(anyhow::anyhow!("Local embedding failed"));
        }

        let vec = unsafe { std::slice::from_raw_parts(ptr, out_dim).to_vec() };

        unsafe {
            remem_ffi::remem_free_embedding(ptr);
        }

        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::new();
        for t in texts {
            results.push(self.embed(t).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

/// Local LLM reasoning provider communicating with an OpenAI-compatible endpoint
/// (such as llama.cpp server, Ollama, or LM Studio) running locally.
pub struct LocalProvider {
    client: reqwest::Client,
    api_base: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

impl LocalProvider {
    /// Create a new Local reasoning provider.
    ///
    /// Reads `LLAMA_API_BASE` or `OLLAMA_API_BASE` from env, defaulting to `http://localhost:8080/v1`.
    pub fn new(api_base: Option<String>) -> Self {
        let api_base = api_base
            .or_else(|| std::env::var("LLAMA_API_BASE").ok())
            .or_else(|| std::env::var("OLLAMA_API_BASE").ok())
            .unwrap_or_else(|| "http://localhost:8080/v1".to_string());

        Self {
            client: reqwest::Client::new(),
            api_base,
        }
    }
}

#[async_trait]
impl Provider for LocalProvider {
    async fn complete(&self, prompt: &str, model: &str) -> anyhow::Result<String> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: prompt.to_string(),
            }],
            max_tokens: 2048,
        };

        let response = super::resiliency::execute_with_retry(
            || {
                self.client
                    .post(format!("{}/chat/completions", self.api_base))
                    .json(&request)
                    .send()
            },
            3,
            std::time::Duration::from_millis(500),
        )
        .await?;

        let resp: ChatResponse = response.json().await?;
        let text = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(text)
    }

    fn name(&self) -> &str {
        "local"
    }
}
