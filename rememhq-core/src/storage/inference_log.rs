//! Persistent audit trail for LLM inference calls.
//!
//! Distinct from two other things that sound similar:
//! - [`crate::providers::CostTracker`] — in-memory running totals only
//!   (no per-call detail, no latency, no errors, doesn't survive a
//!   restart).
//! - [`crate::storage::audit::AuditEntry`] — records memory CRUD
//!   operations (insert/update/delete/archive), not inference calls.
//!
//! An [`InferenceLogEntry`] records one `Provider::complete()`/`chat()`
//! call: which provider and model handled it, how long it took, its
//! token usage if it succeeded, and its error if it didn't. Written by
//! [`crate::providers::tracking::CostTrackingProvider`] after every call.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// One recorded inference call.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferenceLogEntry {
    pub id: Uuid,
    pub provider: String,
    pub model: String,
    /// SHA-256 of the prompt (for `complete()`) or a JSON-serialized
    /// representation of the message list (for `chat()`) — not the
    /// prompt/messages themselves. Lets identical calls be correlated
    /// without storing potentially sensitive content in a queryable
    /// audit table. See [`InferenceLogEntry::hash_prompt`].
    pub prompt_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<usize>,
    pub latency_ms: u64,
    /// Set when the call failed (and so has no usage); `None` on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl InferenceLogEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        prompt_hash: impl Into<String>,
        prompt_tokens: Option<usize>,
        completion_tokens: Option<usize>,
        latency_ms: u64,
        error: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            provider: provider.into(),
            model: model.into(),
            prompt_hash: prompt_hash.into(),
            prompt_tokens,
            completion_tokens,
            latency_ms,
            error,
            timestamp: Utc::now(),
        }
    }

    /// Lowercase-hex SHA-256 of `text` — the actual hashing used for
    /// `prompt_hash`, factored out so callers (and tests) don't each
    /// reimplement it.
    pub fn hash_prompt(text: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

/// Retention policy for inference logs — mirrors
/// [`crate::storage::audit::AuditRetentionPolicy`], but defaults to a
/// shorter window since inference calls are typically far higher-volume
/// than memory CRUD operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceLogRetentionPolicy {
    pub retention_days: u32,
    pub auto_prune: bool,
}

impl Default for InferenceLogRetentionPolicy {
    fn default() -> Self {
        Self {
            retention_days: 30,
            auto_prune: true,
        }
    }
}

impl InferenceLogRetentionPolicy {
    pub fn cutoff_timestamp(&self) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(self.retention_days as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inference_log_entry_creation() {
        let entry = InferenceLogEntry::new(
            "local",
            "phi-3-mini",
            "abc123",
            Some(100),
            Some(50),
            250,
            None,
        );
        assert_eq!(entry.provider, "local");
        assert_eq!(entry.model, "phi-3-mini");
        assert_eq!(entry.prompt_tokens, Some(100));
        assert_eq!(entry.completion_tokens, Some(50));
        assert_eq!(entry.latency_ms, 250);
        assert!(entry.error.is_none());
    }

    #[test]
    fn test_inference_log_entry_error_case_has_no_usage() {
        let entry = InferenceLogEntry::new(
            "anthropic",
            "claude-3-5-sonnet",
            "def456",
            None,
            None,
            5000,
            Some("request timed out".to_string()),
        );
        assert!(entry.prompt_tokens.is_none());
        assert!(entry.completion_tokens.is_none());
        assert_eq!(entry.error.as_deref(), Some("request timed out"));
    }

    #[test]
    fn test_hash_prompt_is_deterministic() {
        let h1 = InferenceLogEntry::hash_prompt("what is the capital of France?");
        let h2 = InferenceLogEntry::hash_prompt("what is the capital of France?");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_hash_prompt_differs_for_different_input() {
        let h1 = InferenceLogEntry::hash_prompt("prompt A");
        let h2 = InferenceLogEntry::hash_prompt("prompt B");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_prompt_matches_known_vector() {
        // Computed independently via Python's hashlib, not copied from
        // this implementation.
        let hash = InferenceLogEntry::hash_prompt("test model content");
        assert_eq!(
            hash,
            "8cf3a78cc64a1d9952a895d574d82ce37ad3b4328893e97dff9611fe3e52706d"
        );
    }

    #[test]
    fn test_retention_policy_default() {
        let policy = InferenceLogRetentionPolicy::default();
        assert_eq!(policy.retention_days, 30);
        assert!(policy.auto_prune);
    }

    #[test]
    fn test_retention_cutoff_is_in_the_past() {
        let policy = InferenceLogRetentionPolicy {
            retention_days: 30,
            auto_prune: true,
        };
        assert!(policy.cutoff_timestamp() < Utc::now());
    }
}
