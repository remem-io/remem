//! Google Gemini provider for reasoning and embedding operations.
//!
//! Reads `GOOGLE_API_KEY` from the environment.
//! Default models: `gemini-2.0-flash` for reasoning/scoring,
//! `text-embedding-004` for embeddings (768 dimensions).

use crate::providers::{EmbeddingProvider, Provider};
use anyhow::{anyhow, Context};
use reqwest::Client;
use serde_json::json;

/// Default reasoning/scoring model for the Google provider.
pub const DEFAULT_REASONING_MODEL: &str = "gemini-2.0-flash";

/// Default embedding model for the Google provider.
pub const DEFAULT_EMBEDDING_MODEL: &str = "text-embedding-004";

/// Embedding dimensions returned by `text-embedding-004`.
pub const EMBEDDING_DIM: usize = 768;

// ---------------------------------------------------------------------------
// GoogleProvider — reasoning / completion
// ---------------------------------------------------------------------------

pub struct GoogleProvider {
    client: Client,
    api_key: String,
}

impl GoogleProvider {
    /// Create a new Google provider.
    ///
    /// Reads `GOOGLE_API_KEY` from the environment if `api_key` is `None`.
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
            DEFAULT_REASONING_MODEL
        } else {
            model
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model_name, self.api_key
        );

        let body = json!({
            "contents": [{"parts": [{"text": prompt}]}]
        });

        let resp = super::resiliency::execute_with_retry(
            || self.client.post(&url).json(&body).send(),
            3,
            std::time::Duration::from_millis(500),
        )
        .await
        .context("Failed to send request to Google API")?;

        let json: serde_json::Value = resp.json().await?;

        // Surface the API error message if the call was rejected
        if let Some(err) = json.get("error") {
            let msg = err["message"].as_str().unwrap_or("unknown error");
            let code = err["code"].as_i64().unwrap_or(0);
            anyhow::bail!("Google API error {}: {}", code, msg);
        }

        let text = json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Unexpected Google response format: {:?}", json))?;

        Ok(text.to_string())
    }

    fn name(&self) -> &str {
        "google"
    }
}

// ---------------------------------------------------------------------------
// GoogleEmbeddings — vector embedding
// ---------------------------------------------------------------------------

pub struct GoogleEmbeddings {
    client: Client,
    api_key: String,
}

impl GoogleEmbeddings {
    /// Create a new Google embeddings provider.
    ///
    /// Reads `GOOGLE_API_KEY` from the environment if `api_key` is `None`.
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
            "https://generativelanguage.googleapis.com/v1beta/models/{}:embedContent?key={}",
            DEFAULT_EMBEDDING_MODEL, self.api_key
        );

        let body = json!({
            "model": format!("models/{}", DEFAULT_EMBEDDING_MODEL),
            "content": {"parts": [{"text": text}]}
        });

        let resp = super::resiliency::execute_with_retry(
            || self.client.post(&url).json(&body).send(),
            3,
            std::time::Duration::from_millis(500),
        )
        .await
        .context("Failed to send embedding request to Google API")?;

        let json: serde_json::Value = resp.json().await?;

        if let Some(err) = json.get("error") {
            let msg = err["message"].as_str().unwrap_or("unknown error");
            let code = err["code"].as_i64().unwrap_or(0);
            anyhow::bail!("Google API error {}: {}", code, msg);
        }

        parse_embedding(&json["embedding"]["values"])
    }

    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:batchEmbedContents?key={}",
            DEFAULT_EMBEDDING_MODEL, self.api_key
        );

        let requests: Vec<serde_json::Value> = texts
            .iter()
            .map(|t| {
                json!({
                    "model": format!("models/{}", DEFAULT_EMBEDDING_MODEL),
                    "content": {"parts": [{"text": t}]}
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

        if let Some(err) = json.get("error") {
            let msg = err["message"].as_str().unwrap_or("unknown error");
            let code = err["code"].as_i64().unwrap_or(0);
            anyhow::bail!("Google API error {}: {}", code, msg);
        }

        let embeddings_json = json["embeddings"]
            .as_array()
            .ok_or_else(|| anyhow!("Unexpected Google batch embedding response format"))?;

        embeddings_json
            .iter()
            .map(|emb| parse_embedding(&emb["values"]))
            .collect()
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_embedding(values: &serde_json::Value) -> anyhow::Result<Vec<f32>> {
    let arr = values
        .as_array()
        .ok_or_else(|| anyhow!("Missing or invalid embedding values in Google response"))?;

    Ok(arr
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_constants() {
        assert_eq!(DEFAULT_REASONING_MODEL, "gemini-2.0-flash");
        assert_eq!(DEFAULT_EMBEDDING_MODEL, "text-embedding-004");
        assert_eq!(EMBEDDING_DIM, 768);
    }

    #[test]
    fn test_google_provider_requires_api_key() {
        std::env::remove_var("GOOGLE_API_KEY");
        assert!(GoogleProvider::new(None).is_err());
        assert!(GoogleEmbeddings::new(None).is_err());
    }

    #[test]
    fn test_google_provider_accepts_explicit_key() {
        let p = GoogleProvider::new(Some("test-key".into()));
        assert!(p.is_ok());
        assert_eq!(p.unwrap().name(), "google");
    }

    #[test]
    fn test_google_embeddings_accepts_explicit_key() {
        let e = GoogleEmbeddings::new(Some("test-key".into()));
        assert!(e.is_ok());
        assert_eq!(e.unwrap().dimension(), EMBEDDING_DIM);
    }

    #[test]
    fn test_parse_embedding_valid() {
        let values = serde_json::json!([0.1, 0.2, 0.3]);
        let emb = parse_embedding(&values).unwrap();
        assert_eq!(emb.len(), 3);
        assert!((emb[0] - 0.1_f32).abs() < 1e-5);
    }

    #[test]
    fn test_parse_embedding_invalid() {
        let values = serde_json::json!(null);
        assert!(parse_embedding(&values).is_err());
    }
}
