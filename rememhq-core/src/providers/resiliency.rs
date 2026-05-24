//! HTTP request resiliency utilities (retries, backoff, jitter).

use reqwest::Response;
use std::time::Duration;

/// Executes a reqwest HTTP request with transient error retries, exponential backoff, and randomized jitter.
///
/// Retries on:
/// - Connection errors, timeouts
/// - 429 Too Many Requests
/// - 500, 502, 503, 504 server errors
pub async fn execute_with_retry<F, Fut>(
    request_fn: F,
    max_retries: usize,
    initial_delay: Duration,
) -> anyhow::Result<Response>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<Response, reqwest::Error>>,
{
    let mut attempt = 0;
    let mut delay = initial_delay;

    loop {
        attempt += 1;
        match request_fn().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return Ok(response);
                }

                let is_transient = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || status == reqwest::StatusCode::INTERNAL_SERVER_ERROR
                    || status == reqwest::StatusCode::BAD_GATEWAY
                    || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
                    || status == reqwest::StatusCode::GATEWAY_TIMEOUT;

                if is_transient && attempt <= max_retries {
                    let mut jitter_ms = 0;
                    if let Ok(duration) =
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    {
                        let nanos = duration.as_nanos();
                        let range = delay.as_millis() / 10; // +/- 10%
                        if range > 0 {
                            let offset = (nanos % (range * 2 + 1)) as i128 - range as i128;
                            jitter_ms = offset;
                        }
                    }
                    let delay_ms = (delay.as_millis() as i128 + jitter_ms).max(1) as u64;
                    let actual_delay = Duration::from_millis(delay_ms);

                    tracing::warn!(
                        "HTTP request failed with status {} (attempt {}/{}). Retrying in {:?}...",
                        status,
                        attempt,
                        max_retries,
                        actual_delay
                    );

                    tokio::time::sleep(actual_delay).await;
                    delay *= 2;
                    continue;
                }

                let text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, text);
            }
            Err(e) => {
                let is_transient = e.is_timeout() || e.is_connect();
                if is_transient && attempt <= max_retries {
                    let mut jitter_ms = 0;
                    if let Ok(duration) =
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    {
                        let nanos = duration.as_nanos();
                        let range = delay.as_millis() / 10;
                        if range > 0 {
                            let offset = (nanos % (range * 2 + 1)) as i128 - range as i128;
                            jitter_ms = offset;
                        }
                    }
                    let delay_ms = (delay.as_millis() as i128 + jitter_ms).max(1) as u64;
                    let actual_delay = Duration::from_millis(delay_ms);

                    tracing::warn!(
                        "Network error: {} (attempt {}/{}). Retrying in {:?}...",
                        e,
                        attempt,
                        max_retries,
                        actual_delay
                    );

                    tokio::time::sleep(actual_delay).await;
                    delay *= 2;
                    continue;
                }
                return Err(e.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, routing::get, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_retry_success_first_time() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let app = Router::new().route("/ok", get(|| async { "ok" }));
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/ok", port);

        let response = execute_with_retry(|| client.get(&url).send(), 3, Duration::from_millis(10))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_retry_on_transient_error_succeeds_eventually() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let app = Router::new().route(
            "/flaky",
            get(move || {
                let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
                async move {
                    if count < 2 {
                        StatusCode::TOO_MANY_REQUESTS
                    } else {
                        StatusCode::OK
                    }
                }
            }),
        );

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/flaky", port);

        let response = execute_with_retry(|| client.get(&url).send(), 3, Duration::from_millis(10))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(call_count.load(Ordering::SeqCst), 3); // 2 fails + 1 success
    }

    #[tokio::test]
    async fn test_retry_fails_on_too_many_transient_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let app = Router::new().route("/fail", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }));

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/fail", port);

        let result =
            execute_with_retry(|| client.get(&url).send(), 2, Duration::from_millis(10)).await;

        assert!(result.is_err());
    }
}
