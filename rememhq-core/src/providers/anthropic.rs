//! Anthropic Claude provider for reasoning operations.

use super::Provider;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Anthropic Claude API client.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
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
    async fn complete(&self, prompt: &str, model: &str) -> anyhow::Result<String> {
        let request = MessagesRequest {
            model: model.to_string(),
            max_tokens: 4096,
            messages: vec![Message {
                role: "user".into(),
                content: prompt.to_string(),
            }],
        };

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        let resp: MessagesResponse = response.json().await?;
        let text = resp
            .content
            .into_iter()
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(text)
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}
