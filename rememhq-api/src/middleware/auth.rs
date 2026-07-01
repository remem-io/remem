//! Bearer token authentication middleware.

use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;

use crate::routes::memories::ErrorResponse;
use rememhq_core::providers::ProviderOptions;

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

        if provided != expected {
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
            let mut options = ProviderOptions::default();
            options.api_key = Some(key_str.to_string());
            return Some(options);
        }
    }
    None
}
