//! Rate limiting middleware — sliding window rate limiter per client IP.

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::net::SocketAddr;
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

/// Determine the key to rate-limit a request by.
///
/// `X-Forwarded-For` is client-supplied and trivially spoofable: without this
/// check, a caller could bypass rate limiting entirely by sending a different
/// (or random) `X-Forwarded-For` value on every request, since each "new" value
/// gets its own bucket. It's only safe to trust when remem is deployed behind a
/// proxy/load balancer that overwrites the header itself (never forwards a
/// client-supplied one), which operators opt into with `REMEM_TRUST_PROXY_HEADERS`.
///
/// By default (not behind a trusted proxy) we key on the actual TCP peer address
/// instead, which the caller cannot forge. If that's unavailable for some reason,
/// we fall back to a single shared bucket rather than trusting an unverifiable
/// header.
fn resolve_client_key(
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    trust_proxy_headers: bool,
) -> String {
    if trust_proxy_headers {
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return forwarded;
        }
    }

    match peer {
        Some(addr) => addr.ip().to_string(),
        None => "global-ip".to_string(),
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

    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let trust_proxy_headers = std::env::var("REMEM_TRUST_PROXY_HEADERS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let client_ip = resolve_client_key(request.headers(), peer, trust_proxy_headers);

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

    fn peer(octets: [u8; 4]) -> SocketAddr {
        SocketAddr::from((octets, 12345))
    }

    fn req_from(path: &str, peer_addr: SocketAddr, forwarded_for: Option<&str>) -> Request {
        let mut builder = Request::builder().uri(path);
        if let Some(ff) = forwarded_for {
            builder = builder.header("x-forwarded-for", ff);
        }
        let mut req = builder.body(Body::empty()).unwrap();
        // Mirrors what `into_make_service_with_connect_info` inserts per-connection
        // in production; we set it explicitly here since `oneshot` bypasses that.
        req.extensions_mut().insert(ConnectInfo(peer_addr));
        req
    }

    #[test]
    fn test_resolve_client_key_uses_peer_address_by_default() {
        let headers = HeaderMap::new();
        assert_eq!(
            resolve_client_key(&headers, Some(peer([1, 2, 3, 4])), false),
            "1.2.3.4"
        );
    }

    #[test]
    fn test_resolve_client_key_ignores_spoofable_header_by_default() {
        // Regression test for the rate-limit bypass: with trust_proxy_headers = false
        // (the default), a caller-supplied X-Forwarded-For must NOT influence the key,
        // even if present — only the real peer address should.
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("9.9.9.9"));
        assert_eq!(
            resolve_client_key(&headers, Some(peer([1, 2, 3, 4])), false),
            "1.2.3.4",
            "a spoofed X-Forwarded-For header must not override the real peer address"
        );
    }

    #[test]
    fn test_resolve_client_key_trusts_header_when_opted_in() {
        // When explicitly configured to trust an upstream proxy, the header takes
        // priority over the (proxy's own) peer address.
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("9.9.9.9"));
        assert_eq!(
            resolve_client_key(&headers, Some(peer([1, 2, 3, 4])), true),
            "9.9.9.9"
        );
    }

    #[test]
    fn test_resolve_client_key_falls_back_to_shared_bucket_without_peer_info() {
        let headers = HeaderMap::new();
        assert_eq!(resolve_client_key(&headers, None, false), "global-ip");
    }

    #[tokio::test]
    async fn test_rate_limiting_by_real_peer_address() {
        let rate_limit_state = Arc::new(Mutex::new(RateLimiterState::new()));
        let app = Router::new()
            .route("/test", get(handle))
            .route("/health", get(handle))
            .layer(axum::middleware::from_fn_with_state(
                rate_limit_state,
                rate_limit_middleware,
            ));

        let client_a = peer([1, 2, 3, 4]);

        // 1. First request
        let req = req_from("/test", client_a, None);
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let headers = response.headers();
        assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "100");
        assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "99");

        // 2. Healthcheck bypass
        let req = req_from("/health", client_a, None);
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("X-RateLimit-Limit").is_none());

        // 3. Exceed the limit (make 99 more requests from the same peer)
        for i in 0..99 {
            let req = req_from("/test", client_a, None);
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let remaining = 98 - i;
            assert_eq!(
                res.headers().get("X-RateLimit-Remaining").unwrap(),
                &remaining.to_string()
            );
        }

        // 4. The 101st request should be rate limited (429)
        let req = req_from("/test", client_a, None);
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(res.headers().contains_key("Retry-After"));
        assert_eq!(res.headers().get("X-RateLimit-Remaining").unwrap(), "0");

        // 5. Request from a different real peer should still be allowed
        let client_b = peer([5, 6, 7, 8]);
        let req = req_from("/test", client_b, None);
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get("X-RateLimit-Remaining").unwrap(), "99");
    }

    #[tokio::test]
    async fn test_spoofed_x_forwarded_for_does_not_bypass_rate_limit() {
        // Regression test: the same real peer sends 100 requests, each with a
        // *different* X-Forwarded-For value. Before the fix, every request would
        // land in its own bucket (keyed by the header) and the limit would never
        // trigger. Now the header is ignored by default, so they must all share
        // one bucket keyed by the real peer address and get rate limited.
        let rate_limit_state = Arc::new(Mutex::new(RateLimiterState::new()));
        let app = Router::new()
            .route("/test", get(handle))
            .layer(axum::middleware::from_fn_with_state(
                rate_limit_state,
                rate_limit_middleware,
            ));

        let attacker = peer([10, 0, 0, 1]);

        for i in 0..100 {
            let spoofed_ip = format!("{}.{}.{}.{}", i, i, i, i);
            let req = req_from("/test", attacker, Some(&spoofed_ip));
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "request {i} should succeed");
        }

        // The 101st request, still with yet another spoofed header, must now be
        // rate limited since it shares the attacker's real-peer-address bucket.
        let req = req_from("/test", attacker, Some("255.255.255.255"));
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "spoofing X-Forwarded-For must not bypass the rate limit"
        );
    }
}
