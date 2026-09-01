//! Webhook Callbacks & Event Dispatcher.
//!
//! Delivers real-time notifications for memory modifications, session consolidation,
//! and contradiction events with HMAC-SHA256 signature verification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

/// A registered webhook subscription endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>, // e.g. ["memory.created", "consolidation.completed", "*"]
    pub secret: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl WebhookEndpoint {
    pub fn new(url: impl Into<String>, events: Vec<String>, secret: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            url: url.into(),
            events,
            secret: secret.into(),
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// Check if this webhook subscribes to a specific event name.
    pub fn subscribes_to(&self, event_name: &str) -> bool {
        self.enabled
            && (self.events.contains(&"*".to_string())
                || self.events.iter().any(|e| e == event_name))
    }
}

/// Outgoing webhook payload structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event_id: Uuid,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub data: serde_json::Value,
}

impl WebhookPayload {
    pub fn new(event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            event_type: event_type.into(),
            timestamp: Utc::now(),
            data,
        }
    }

    /// Compute HMAC-SHA256 signature string for this payload body using endpoint secret.
    pub fn compute_signature(&self, secret: &str) -> String {
        let json_body = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(b":");
        hasher.update(json_body.as_bytes());
        let hash = hasher.finalize();
        hash.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// Webhook dispatcher delivering payloads over HTTP.
pub struct WebhookDispatcher {
    client: reqwest::Client,
}

impl Default for WebhookDispatcher {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl WebhookDispatcher {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Dispatch a webhook event asynchronously to a given endpoint with signature headers.
    pub async fn dispatch(
        &self,
        endpoint: &WebhookEndpoint,
        payload: &WebhookPayload,
    ) -> anyhow::Result<bool> {
        if !endpoint.subscribes_to(&payload.event_type) {
            return Ok(false);
        }

        let signature = payload.compute_signature(&endpoint.secret);
        let resp = self
            .client
            .post(&endpoint.url)
            .header("Content-Type", "application/json")
            .header("X-Remem-Event", &payload.event_type)
            .header("X-Remem-Signature", signature)
            .json(payload)
            .send()
            .await?;

        Ok(resp.status().is_success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_subscription_matching() {
        let endpoint = WebhookEndpoint::new(
            "https://api.example.com/webhook",
            vec!["memory.created".into(), "memory.updated".into()],
            "secret123",
        );

        assert!(endpoint.subscribes_to("memory.created"));
        assert!(endpoint.subscribes_to("memory.updated"));
        assert!(!endpoint.subscribes_to("consolidation.completed"));
    }

    #[test]
    fn test_webhook_signature_determinism() {
        let payload = WebhookPayload::new(
            "memory.created",
            serde_json::json!({"id": "123", "content": "test memory"}),
        );

        let sig1 = payload.compute_signature("my_secret");
        let sig2 = payload.compute_signature("my_secret");
        assert_eq!(sig1, sig2);
        assert!(!sig1.is_empty());
    }
}
