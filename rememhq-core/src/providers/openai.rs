//! OpenAI provider for reasoning operations.

use super::{ChatMessage, ChatResponse, ChatRole, Provider, ProviderOptions, Tool, ToolCall};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

/// OpenAI API client.
pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    ///
    /// Reads `OPENAI_API_KEY` from environment if not provided.
    pub fn new(api_key: Option<String>) -> anyhow::Result<Self> {
        let api_key = api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.openai.com".into(),
        })
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn complete(
        &self,
        prompt: &str,
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
        let messages = vec![ChatMessage {
            role: ChatRole::User,
            content: prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];
        let resp = self.chat(&messages, &[], model, options).await?;
        Ok((resp.message.content, resp.usage))
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[Tool],
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<ChatResponse> {
        let mut openai_messages = Vec::new();

        for msg in messages {
            let role_str = match msg.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "tool",
            };

            let mut msg_json = json!({
                "role": role_str,
                "content": msg.content
            });

            if let Some(tool_calls) = &msg.tool_calls {
                let mut openai_tool_calls = Vec::new();
                for tc in tool_calls {
                    openai_tool_calls.push(json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments.to_string()
                        }
                    }));
                }
                msg_json["tool_calls"] = json!(openai_tool_calls);
            }

            if let Some(tool_call_id) = &msg.tool_call_id {
                msg_json["tool_call_id"] = json!(tool_call_id);
            }

            openai_messages.push(msg_json);
        }

        let mut request = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": openai_messages
        });

        if !tools.is_empty() {
            let mut openai_tools = Vec::new();
            for t in tools {
                openai_tools.push(json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    }
                }));
            }
            request["tools"] = json!(openai_tools);
        }

        let active_api_key = options
            .and_then(|o| o.api_key.as_deref())
            .unwrap_or(&self.api_key)
            .to_string();

        let response = super::resiliency::execute_with_retry(
            || {
                self.client
                    .post(format!("{}/v1/chat/completions", self.base_url))
                    .header("Authorization", format!("Bearer {}", active_api_key))
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
            return Err(anyhow::anyhow!("OpenAI API error {}: {}", status, text));
        }

        let resp: Value = response.json().await?;

        let choice = resp["choices"][0]["message"].clone();

        let content = choice["content"].as_str().unwrap_or_default().to_string();

        let mut parsed_tool_calls = Vec::new();
        if let Some(tcs) = choice["tool_calls"].as_array() {
            for tc in tcs {
                let id = tc["id"].as_str().unwrap_or_default().to_string();
                let function = &tc["function"];
                let name = function["name"].as_str().unwrap_or_default().to_string();
                let arguments_str = function["arguments"].as_str().unwrap_or("{}");
                let arguments: Value = serde_json::from_str(arguments_str).unwrap_or(json!({}));

                parsed_tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }

        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content,
            tool_calls: if parsed_tool_calls.is_empty() {
                None
            } else {
                Some(parsed_tool_calls)
            },
            tool_call_id: None,
        };

        let usage = resp.get("usage").map(|u| crate::providers::TokenUsage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as usize,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as usize,
        });

        Ok(ChatResponse {
            message: msg,
            usage,
        })
    }

    fn name(&self) -> &str {
        "openai"
    }
}
