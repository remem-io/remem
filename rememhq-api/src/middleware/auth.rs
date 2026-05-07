//! Bearer token authentication middleware.

use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;

use crate::routes::memories::ErrorResponse;

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
