use crate::providers::{EmbeddingProvider, Provider};
use anyhow::{anyhow, Context};
use reqwest::Client;
use serde_json::json;

pub struct GoogleProvider {
    client: Client,
    api_key: String,
}

impl GoogleProvider {
    pub fn new(api_key: Option<String>) -> anyhow::Result<Self> {
        let key = api_key
            .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
            .ok_or_else(|| anyhow!("GOOGLE_API_KEY must be set"))?;

        Ok(Self {
            client: Client::new(),
            api_key: key,
        })
    }
}

#[async_trait::async_trait]
impl Provider for GoogleProvider {
    async fn complete(&self, prompt: &str, model: &str) -> anyhow::Result<String> {
        let model_name = if model.is_empty() {
            "gemini-1.5-flash"
        } else {
            model
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model_name, self.api_key
        );

        let body = json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }]
        });

        let resp = super::resiliency::execute_with_retry(
            || self.client.post(&url).json(&body).send(),
            3,
            std::time::Duration::from_millis(500),
        )
        .await
        .context("Failed to send request to Google API")?;

        let json: serde_json::Value = resp.json().await?;
        let text = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Unexpected Google response format"))?;

        Ok(text.to_string())
    }

    fn name(&self) -> &str {
        "google"
    }
}

pub struct GoogleEmbeddings {
    #[allow(dead_code)]
    client: Client,
    #[allow(dead_code)]
    api_key: String,
}

impl GoogleEmbeddings {
    pub fn new(api_key: Option<String>) -> anyhow::Result<Self> {
        let key = api_key
            .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
            .ok_or_else(|| anyhow!("GOOGLE_API_KEY must be set"))?;

        Ok(Self {
            client: Client::new(),
            api_key: key,
        })
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for GoogleEmbeddings {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent?key={}",
            self.api_key
        );

        let body = json!({
            "model": "models/text-embedding-004",
            "content": {
                "parts": [{"text": text}]
            }
        });

        let resp = super::resiliency::execute_with_retry(
            || self.client.post(&url).json(&body).send(),
            3,
            std::time::Duration::from_millis(500),
        )
        .await
        .context("Failed to send embedding request to Google API")?;

        let json: serde_json::Value = resp.json().await?;
        let values = json["embedding"]["values"]
            .as_array()
            .ok_or_else(|| anyhow!("Unexpected Google embedding response format"))?;

        let embedding: Vec<f32> = values
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();

        Ok(embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:batchEmbedContents?key={}",
            self.api_key
        );

        let requests: Vec<serde_json::Value> = texts
            .iter()
            .map(|t| {
                json!({
                    "model": "models/text-embedding-004",
                    "content": { "parts": [{"text": t}] }
                })
            })
            .collect();

        let body = json!({ "requests": requests });

        let resp = super::resiliency::execute_with_retry(
            || self.client.post(&url).json(&body).send(),
            3,
            std::time::Duration::from_millis(500),
        )
        .await
        .context("Failed to send batch embedding request to Google API")?;

        let json: serde_json::Value = resp.json().await?;
        let embeddings_json = json["embeddings"]
            .as_array()
            .ok_or_else(|| anyhow!("Unexpected Google batch embedding response format"))?;

        let mut results = Vec::with_capacity(texts.len());
        for emb_json in embeddings_json {
            let values = emb_json["values"]
                .as_array()
                .ok_or_else(|| anyhow!("Missing values in batch embedding response"))?;

            let embedding: Vec<f32> = values
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();

            results.push(embedding);
        }

        Ok(results)
    }

    fn dimension(&self) -> usize {
        768
    }
}
