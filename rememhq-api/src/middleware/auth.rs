//! Bearer token authentication middleware.

use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;

use crate::routes::memories::ErrorResponse;
use rememhq_core::providers::ProviderOptions;

/// Compare two strings without leaking information about where the mismatch
/// occurs, via timing.
///
/// A plain `a != b` short-circuits on the first differing byte, so how long
/// the comparison takes depends on how many leading bytes of `provided`
/// happen to match `expected`. Measured over enough requests, that timing
/// difference can be used to recover the secret byte by byte — the classic
/// timing side-channel on secret comparisons (API keys, tokens, HMACs, etc).
/// This walks every byte regardless of where a mismatch is found.
///
/// Like most constant-time compare implementations, this still leaks the
/// *length* of `expected` via the early return below. That's an accepted
/// tradeoff here since API keys have a fixed, non-secret format/length.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Check the Authorization header against the REMEM_API_KEY env var.
///
/// If REMEM_API_KEY is not set, all requests are allowed (dev mode).
pub fn check_auth(headers: &HeaderMap) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Ok(expected) = std::env::var("REMEM_API_KEY") {
        let provided = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");

        if !constant_time_eq(provided, &expected) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Invalid API key".into(),
                }),
            ));
        }
    }
    Ok(())
}

/// Extract provider options (e.g. API keys) from request headers.
pub fn extract_provider_options(headers: &HeaderMap) -> Option<ProviderOptions> {
    if let Some(key) = headers.get("x-llm-api-key") {
        if let Ok(key_str) = key.to_str() {
            return Some(ProviderOptions {
                api_key: Some(key_str.to_string()),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // See rememhq-core/src/config.rs tests for why this lock exists:
    // std::env::set_var/remove_var mutate process-wide global state, and
    // cargo test runs tests in parallel by default.
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_constant_time_eq_equal_strings() {
        assert!(constant_time_eq("secret-key-123", "secret-key-123"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn test_constant_time_eq_different_content_same_length() {
        assert!(!constant_time_eq("secret-key-123", "secret-key-124"));
        // Differs in the first byte rather than the last — the whole point of
        // this function is that this isn't any faster to reject than the case
        // above, but we can at least verify it's still correctly rejected.
        assert!(!constant_time_eq("Xecret-key-123", "secret-key-123"));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        assert!(!constant_time_eq("short", "much-longer-value"));
        assert!(!constant_time_eq("", "nonempty"));
    }

    #[test]
    fn test_check_auth_rejects_wrong_key() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("REMEM_API_KEY", "correct-key");
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer wrong-key".parse().unwrap());
        assert!(check_auth(&headers).is_err());
        std::env::remove_var("REMEM_API_KEY");
    }

    #[test]
    fn test_check_auth_accepts_correct_key() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("REMEM_API_KEY", "correct-key");
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer correct-key".parse().unwrap());
        assert!(check_auth(&headers).is_ok());
        std::env::remove_var("REMEM_API_KEY");
    }

    #[test]
    fn test_check_auth_allows_all_when_no_key_configured() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::remove_var("REMEM_API_KEY");
        let headers = HeaderMap::new();
        assert!(check_auth(&headers).is_ok());
    }
}
