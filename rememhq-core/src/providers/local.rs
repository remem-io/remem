use crate::providers::{EmbeddingProvider, Provider, ProviderOptions};
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
    async fn embed(&self, text: &str, _options: Option<&ProviderOptions>) -> anyhow::Result<Vec<f32>> {
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

    async fn embed_batch(&self, texts: &[String], _options: Option<&ProviderOptions>) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::new();
        for t in texts {
            results.push(self.embed(t, _options).await?);
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
    async fn complete(&self, prompt: &str, model: &str, _options: Option<&ProviderOptions>) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
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

        Ok((text, None))
    }

    async fn chat(
        &self,
        messages: &[crate::providers::ChatMessage],
        tools: &[crate::providers::Tool],
        model: &str,
        _options: Option<&ProviderOptions>,
    ) -> anyhow::Result<crate::providers::ChatResponse> {
        let mut openai_messages = Vec::new();

        for msg in messages {
            let role_str = match msg.role {
                crate::providers::ChatRole::System => "system",
                crate::providers::ChatRole::User => "user",
                crate::providers::ChatRole::Assistant => "assistant",
                crate::providers::ChatRole::Tool => "tool",
            };

            let mut msg_json = serde_json::json!({
                "role": role_str,
                "content": msg.content
            });

            if let Some(tool_calls) = &msg.tool_calls {
                let mut openai_tool_calls = Vec::new();
                for tc in tool_calls {
                    openai_tool_calls.push(serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments.to_string()
                        }
                    }));
                }
                msg_json["tool_calls"] = serde_json::json!(openai_tool_calls);
            }

            if let Some(tool_call_id) = &msg.tool_call_id {
                msg_json["tool_call_id"] = serde_json::json!(tool_call_id);
            }

            openai_messages.push(msg_json);
        }

        let mut request = serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "messages": openai_messages
        });

        if !tools.is_empty() {
            let mut openai_tools = Vec::new();
            for t in tools {
                openai_tools.push(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    }
                }));
            }
            request["tools"] = serde_json::json!(openai_tools);
        }

        let response = super::resiliency::execute_with_retry(
            || {
                self.client
                    .post(format!("{}/chat/completions", self.api_base))
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
            },
            3,
            std::time::Duration::from_millis(500),
        )
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Local API error {}: {}", status, text));
        }

        let resp: serde_json::Value = response.json().await?;

        let choice = resp["choices"][0]["message"].clone();
        let content = choice["content"].as_str().unwrap_or_default().to_string();

        let mut parsed_tool_calls = Vec::new();
        if let Some(tcs) = choice["tool_calls"].as_array() {
            for tc in tcs {
                let id = tc["id"].as_str().unwrap_or_default().to_string();
                let function = &tc["function"];
                let name = function["name"].as_str().unwrap_or_default().to_string();
                let arguments_str = function["arguments"].as_str().unwrap_or("{}");
                let arguments: serde_json::Value =
                    serde_json::from_str(arguments_str).unwrap_or(serde_json::json!({}));

                parsed_tool_calls.push(crate::providers::ToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }

        let msg = crate::providers::ChatMessage {
            role: crate::providers::ChatRole::Assistant,
            content,
            tool_calls: if parsed_tool_calls.is_empty() {
                None
            } else {
                Some(parsed_tool_calls)
            },
            tool_call_id: None,
        };

        Ok(crate::providers::ChatResponse { message: msg, usage: None })
    }

    fn name(&self) -> &str {
        "local"
    }
}
