//! Rate limiting middleware — sliding window rate limiter per client IP.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Sliding-window rate limiter state.
pub struct RateLimiterState {
    requests: HashMap<String, Vec<Instant>>,
}

impl RateLimiterState {
    /// Create a new sliding-window rate limiter state.
    pub fn new() -> Self {
        Self {
            requests: HashMap::new(),
        }
    }
}

impl Default for RateLimiterState {
    fn default() -> Self {
        Self::new()
    }
}

/// Axum rate limit middleware.
pub async fn rate_limit_middleware(
    State(state): State<Arc<Mutex<RateLimiterState>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip rate limiting for the healthcheck endpoint
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let headers = request.headers();

    // Extract IP from X-Forwarded-For header, fallback to global key
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .unwrap_or_else(|| "global-ip".to_string());

    let now = Instant::now();
    let window = Duration::from_secs(60);
    let max_requests = 100;

    let mut state = state.lock().await;
    let timestamps = state.requests.entry(client_ip).or_default();

    // Filter out timestamps outside the sliding window
    timestamps.retain(|&t| now.duration_since(t) < window);

    let current_count = timestamps.len();

    if current_count >= max_requests {
        let oldest = timestamps.first().copied().unwrap_or(now);
        let elapsed = now.duration_since(oldest);
        let retry_after_secs = window.as_secs().saturating_sub(elapsed.as_secs());

        let mut headers = HeaderMap::new();
        headers.insert("Retry-After", HeaderValue::from(retry_after_secs));
        headers.insert("X-RateLimit-Limit", HeaderValue::from(max_requests));
        headers.insert("X-RateLimit-Remaining", HeaderValue::from(0));

        let body = serde_json::json!({
            "error": "Too Many Requests",
            "message": format!("Rate limit of {} requests per minute exceeded. Please try again later.", max_requests)
        });

        let mut res = Response::new(Body::from(body.to_string()));
        *res.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        res.headers_mut().extend(headers);
        res.headers_mut()
            .insert("Content-Type", HeaderValue::from_static("application/json"));
        return Ok(res);
    }

    timestamps.push(now);
    let remaining = max_requests - timestamps.len();

    drop(state); // Release lock before proceeding

    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    headers.insert("X-RateLimit-Limit", HeaderValue::from(max_requests));
    headers.insert("X-RateLimit-Remaining", HeaderValue::from(remaining));

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use tower::ServiceExt;

    async fn handle() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn test_rate_limiting_middleware() {
        let rate_limit_state = Arc::new(Mutex::new(RateLimiterState::new()));
        let app = Router::new()
            .route("/test", get(handle))
            .route("/health", get(handle))
            .layer(axum::middleware::from_fn_with_state(
                rate_limit_state,
                rate_limit_middleware,
            ));

        // 1. First request
        let req = Request::builder()
            .uri("/test")
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let headers = response.headers();
        assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "100");
        assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "99");

        // 2. Healthcheck bypass
        let req = Request::builder()
            .uri("/health")
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("X-RateLimit-Limit").is_none());

        // 3. Exceed the limit (make 99 more requests from 1.2.3.4)
        for i in 0..99 {
            let req = Request::builder()
                .uri("/test")
                .header("x-forwarded-for", "1.2.3.4")
                .body(Body::empty())
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let remaining = 98 - i;
            assert_eq!(
                res.headers().get("X-RateLimit-Remaining").unwrap(),
                &remaining.to_string()
            );
        }

        // 4. The 101st request should be rate limited (429)
        let req = Request::builder()
            .uri("/test")
            .header("x-forwarded-for", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(res.headers().contains_key("Retry-After"));
        assert_eq!(res.headers().get("X-RateLimit-Remaining").unwrap(), "0");

        // 5. Request from another IP should still be allowed
        let req = Request::builder()
            .uri("/test")
            .header("x-forwarded-for", "5.6.7.8")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("X-RateLimit-Remaining").unwrap(), "99");
    }
}
