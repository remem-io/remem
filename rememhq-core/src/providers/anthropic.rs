//! Anthropic Claude provider for reasoning operations.

use super::{ChatMessage, ChatResponse, ChatRole, Provider, ProviderOptions, Tool, ToolCall};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

/// Anthropic Claude API client.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// Reads `ANTHROPIC_API_KEY` from environment if not provided.
    pub fn new(api_key: Option<String>) -> anyhow::Result<Self> {
        let api_key = api_key
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.anthropic.com".into(),
        })
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
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
        let mut system_prompt = String::new();
        let mut anthropic_messages = Vec::new();

        for msg in messages {
            match msg.role {
                ChatRole::System => {
                    if !system_prompt.is_empty() {
                        system_prompt.push('\n');
                    }
                    system_prompt.push_str(&msg.content);
                }
                ChatRole::User => {
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        anthropic_messages.push(json!({
                            "role": "user",
                            "content": [
                                {
                                    "type": "tool_result",
                                    "tool_use_id": tool_call_id,
                                    "content": msg.content.clone()
                                }
                            ]
                        }));
                    } else {
                        anthropic_messages.push(json!({
                            "role": "user",
                            "content": msg.content.clone()
                        }));
                    }
                }
                ChatRole::Assistant => {
                    if let Some(tool_calls) = &msg.tool_calls {
                        let mut content_blocks = Vec::new();
                        if !msg.content.is_empty() {
                            content_blocks.push(json!({
                                "type": "text",
                                "text": msg.content.clone()
                            }));
                        }
                        for tc in tool_calls {
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": tc.arguments
                            }));
                        }
                        anthropic_messages.push(json!({
                            "role": "assistant",
                            "content": content_blocks
                        }));
                    } else {
                        anthropic_messages.push(json!({
                            "role": "assistant",
                            "content": msg.content.clone()
                        }));
                    }
                }
                ChatRole::Tool => {
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        anthropic_messages.push(json!({
                            "role": "user",
                            "content": [
                                {
                                    "type": "tool_result",
                                    "tool_use_id": tool_call_id,
                                    "content": msg.content.clone()
                                }
                            ]
                        }));
                    }
                }
            }
        }

        let mut request = json!({
            "model": model,
            "max_tokens": 4096,
            "messages": anthropic_messages
        });

        if !system_prompt.is_empty() {
            request["system"] = json!([{
                "type": "text",
                "text": system_prompt,
                "cache_control": { "type": "ephemeral" }
            }]);
        }

        if !tools.is_empty() {
            let mut anthropic_tools = Vec::new();
            for (i, t) in tools.iter().enumerate() {
                let mut tool_json = json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema
                });
                
                // Add cache control to the last tool
                if i == tools.len() - 1 {
                    if let Some(obj) = tool_json.as_object_mut() {
                        obj.insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
                    }
                }
                
                anthropic_tools.push(tool_json);
            }
            request["tools"] = json!(anthropic_tools);
        }

        let active_api_key = options
            .and_then(|o| o.api_key.as_deref())
            .unwrap_or(&self.api_key)
            .to_string();

        let response = super::resiliency::execute_with_retry(
            || {
                self.client
                    .post(format!("{}/v1/messages", self.base_url))
                    .header("x-api-key", &active_api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("anthropic-beta", "prompt-caching-2024-09-02")
                    .header("content-type", "application/json")
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
            return Err(anyhow::anyhow!("Anthropic API error {}: {}", status, text));
        }

        let resp: Value = response.json().await?;

        let mut final_content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(content_arr) = resp["content"].as_array() {
            for block in content_arr {
                if block["type"] == "text" {
                    if let Some(t) = block["text"].as_str() {
                        final_content.push_str(t);
                    }
                } else if block["type"] == "tool_use" {
                    tool_calls.push(ToolCall {
                        id: block["id"].as_str().unwrap_or_default().to_string(),
                        name: block["name"].as_str().unwrap_or_default().to_string(),
                        arguments: block["input"].clone(),
                    });
                }
            }
        }

        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content: final_content,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
        };

        let usage = resp.get("usage").map(|u| {
            let prompt_tokens = u["input_tokens"].as_u64().unwrap_or(0) as usize;
            let completion_tokens = u["output_tokens"].as_u64().unwrap_or(0) as usize;
            crate::providers::TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }
        });

        Ok(ChatResponse {
            message: msg,
            usage,
        })
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}
