//! HTTP request resiliency utilities: exponential backoff with full jitter, circuit breaker, and retry loops.

use reqwest::Response;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CircuitState {
    Closed,
    HalfOpen,
    Open,
}

/// Production-grade Circuit Breaker to prevent cascading failures to down LLM providers.
pub struct CircuitBreaker {
    failure_threshold: usize,
    recovery_timeout: Duration,
    consecutive_failures: AtomicUsize,
    last_failure_time: RwLock<Option<Instant>>,
    success_threshold_in_half_open: usize,
    consecutive_successes: AtomicUsize,
    total_trips: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, recovery_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            consecutive_failures: AtomicUsize::new(0),
            last_failure_time: RwLock::new(None),
            success_threshold_in_half_open: 2,
            consecutive_successes: AtomicUsize::new(0),
            total_trips: AtomicU64::new(0),
        }
    }

    /// Default circuit breaker (trips after 5 consecutive failures, 30s recovery timeout).
    pub fn standard() -> Self {
        Self::new(5, Duration::from_secs(30))
    }

    /// Current state of the circuit breaker.
    pub fn state(&self) -> CircuitState {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < self.failure_threshold {
            return CircuitState::Closed;
        }

        let last_time = self.last_failure_time.read().ok().and_then(|t| *t);
        if let Some(last) = last_time {
            if last.elapsed() >= self.recovery_timeout {
                return CircuitState::HalfOpen;
            }
        }

        CircuitState::Open
    }

    /// Check if a request is permitted to proceed.
    pub fn allow_request(&self) -> bool {
        match self.state() {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open => false,
        }
    }

    /// Record a successful request.
    pub fn record_success(&self) {
        if self.state() == CircuitState::HalfOpen {
            let successes = self.consecutive_successes.fetch_add(1, Ordering::SeqCst) + 1;
            if successes >= self.success_threshold_in_half_open {
                // Reset circuit breaker to Closed
                self.consecutive_failures.store(0, Ordering::SeqCst);
                self.consecutive_successes.store(0, Ordering::SeqCst);
                if let Ok(mut lock) = self.last_failure_time.write() {
                    *lock = None;
                }
            }
        } else {
            self.consecutive_failures.store(0, Ordering::Relaxed);
        }
    }

    /// Record a failed request.
    pub fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut lock) = self.last_failure_time.write() {
            *lock = Some(Instant::now());
        }
        if failures == self.failure_threshold {
            self.total_trips.fetch_add(1, Ordering::Relaxed);
            tracing::error!("Circuit breaker tripped to OPEN state after {} consecutive failures", failures);
        }
    }

    /// Total number of times the circuit has tripped to Open.
    pub fn trips(&self) -> u64 {
        self.total_trips.load(Ordering::Relaxed)
    }
}

/// Calculate backoff duration with Decorrelated Full Jitter (AWS recommendation).
pub fn calculate_jitter_delay(attempt: usize, base_delay: Duration, max_delay: Duration) -> Duration {
    let max_millis = (base_delay.as_millis() as u64)
        .saturating_mul(1u64.checked_shl(attempt.min(10) as u32).unwrap_or(u64::MAX));
    let capped_millis = max_millis.min(max_delay.as_millis() as u64).max(1);

    let mut pseudo_random = 0u64;
    if let Ok(time) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        pseudo_random = (time.as_nanos() % capped_millis as u128) as u64;
    }
    Duration::from_millis(pseudo_random.max(base_delay.as_millis() as u64 / 2))
}

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
    let max_delay = Duration::from_secs(10);

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
                    let actual_delay = calculate_jitter_delay(attempt, initial_delay, max_delay);

                    tracing::warn!(
                        "HTTP request failed with status {} (attempt {}/{}). Retrying in {:?}...",
                        status,
                        attempt,
                        max_retries,
                        actual_delay
                    );

                    tokio::time::sleep(actual_delay).await;
                    continue;
                }

                let text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, text);
            }
            Err(e) => {
                let is_transient = e.is_timeout() || e.is_connect();
                if is_transient && attempt <= max_retries {
                    let actual_delay = calculate_jitter_delay(attempt, initial_delay, max_delay);

                    tracing::warn!(
                        "Network error: {} (attempt {}/{}). Retrying in {:?}...",
                        e,
                        attempt,
                        max_retries,
                        actual_delay
                    );

                    tokio::time::sleep(actual_delay).await;
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

    #[test]
    fn test_circuit_breaker_flow() {
        let cb = CircuitBreaker::new(2, Duration::from_millis(20));
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
        assert_eq!(cb.trips(), 1);

        // Sleep past recovery timeout
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.allow_request());

        // In HalfOpen, consecutive successes reset it to Closed
        cb.record_success();
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

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
