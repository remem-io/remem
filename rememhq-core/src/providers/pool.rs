//! Provider connection pool, concurrency limiter, and token/cost metering.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Usage statistics and cost estimate for LLM calls.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct CostSummary {
    pub total_calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: f64,
    pub usage_by_provider: HashMap<String, u64>,
}

/// Thread-safe token usage and financial cost tracking.
#[derive(Debug, Default)]
pub struct CostTracker {
    total_calls: AtomicU64,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
    cost_micros: AtomicU64, // Cost in micro-dollars ($0.000001) for integer atomic safety
    provider_calls: std::sync::RwLock<HashMap<String, u64>>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record token consumption and calculate estimated USD cost.
    pub fn record_usage(
        &self,
        provider: &str,
        model: &str,
        prompt: usize,
        completion: usize,
    ) -> f64 {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.prompt_tokens
            .fetch_add(prompt as u64, Ordering::Relaxed);
        self.completion_tokens
            .fetch_add(completion as u64, Ordering::Relaxed);

        if let Ok(mut map) = self.provider_calls.write() {
            *map.entry(provider.to_string()).or_insert(0) += 1;
        }

        let cost_usd = estimate_cost(model, prompt, completion);
        let cost_micro = (cost_usd * 1_000_000.0) as u64;
        self.cost_micros.fetch_add(cost_micro, Ordering::Relaxed);

        cost_usd
    }

    /// Retrieve aggregate metrics snapshot.
    pub fn summary(&self) -> CostSummary {
        let total_calls = self.total_calls.load(Ordering::Relaxed);
        let prompt_tokens = self.prompt_tokens.load(Ordering::Relaxed);
        let completion_tokens = self.completion_tokens.load(Ordering::Relaxed);
        let cost_micros = self.cost_micros.load(Ordering::Relaxed);
        let usage_by_provider = self
            .provider_calls
            .read()
            .map(|m| m.clone())
            .unwrap_or_default();

        CostSummary {
            total_calls,
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            estimated_cost_usd: (cost_micros as f64) / 1_000_000.0,
            usage_by_provider,
        }
    }
}

/// Calculate estimated API cost for standard frontier models.
pub fn estimate_cost(model: &str, prompt_tokens: usize, completion_tokens: usize) -> f64 {
    let model_lower = model.to_lowercase();
    let (prompt_rate_per_m, comp_rate_per_m) = match model_lower.as_str() {
        // OpenAI
        m if m.contains("gpt-4o-mini") => (0.15, 0.60),
        m if m.contains("gpt-4o") => (2.50, 10.00),
        m if m.contains("text-embedding-3-small") => (0.02, 0.0),
        m if m.contains("text-embedding-3-large") => (0.13, 0.0),
        // Anthropic
        m if m.contains("claude-3-5-sonnet") || m.contains("claude-3-7-sonnet") => (3.00, 15.00),
        m if m.contains("claude-3-5-haiku") || m.contains("claude-3-haiku") => (0.80, 4.00),
        m if m.contains("claude-3-opus") => (15.00, 75.00),
        // Google Gemini
        m if m.contains("gemini-1.5-pro") || m.contains("gemini-2.0-pro") => (1.25, 5.00),
        m if m.contains("gemini-1.5-flash") || m.contains("gemini-2.0-flash") => (0.075, 0.30),
        m if m.contains("text-embedding-004") => (0.00, 0.0), // Free tier / negligible
        _ => (1.00, 3.00),                                    // Fallback baseline
    };

    (prompt_tokens as f64 * prompt_rate_per_m / 1_000_000.0)
        + (completion_tokens as f64 * comp_rate_per_m / 1_000_000.0)
}

/// Provider connection manager with concurrency bounds and client pooling.
pub struct ProviderPool {
    client: reqwest::Client,
    concurrency_limit: Arc<Semaphore>,
    active_requests: AtomicUsize,
    pub cost_tracker: Arc<CostTracker>,
}

impl ProviderPool {
    /// Create a new connection pool with custom concurrency bounds.
    pub fn new(max_concurrent_requests: usize) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()?;

        Ok(Self {
            client,
            concurrency_limit: Arc::new(Semaphore::new(max_concurrent_requests.max(1))),
            active_requests: AtomicUsize::new(0),
            cost_tracker: Arc::new(CostTracker::new()),
        })
    }

    /// Default pool with 20 concurrent in-flight requests.
    pub fn default_pool() -> anyhow::Result<Self> {
        Self::new(20)
    }

    /// Access the shared HTTP client.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Acquire a concurrency permit for an outgoing LLM request.
    pub async fn acquire_permit(&self) -> anyhow::Result<tokio::sync::OwnedSemaphorePermit> {
        let permit = self
            .concurrency_limit
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to acquire provider pool permit: {}", e))?;
        self.active_requests.fetch_add(1, Ordering::Relaxed);
        Ok(permit)
    }

    /// Report completion of an active request.
    pub fn release_active(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Current number of in-flight provider requests.
    pub fn in_flight(&self) -> usize {
        self.active_requests.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker_accumulation() {
        let tracker = CostTracker::new();
        let cost1 = tracker.record_usage("openai", "gpt-4o-mini", 1000, 500);
        assert!(cost1 > 0.0);

        let cost2 = tracker.record_usage("anthropic", "claude-3-5-sonnet", 2000, 1000);
        assert!(cost2 > 0.0);

        let summary = tracker.summary();
        assert_eq!(summary.total_calls, 2);
        assert_eq!(summary.prompt_tokens, 3000);
        assert_eq!(summary.completion_tokens, 1500);
        assert_eq!(summary.total_tokens, 4500);
        assert!(summary.estimated_cost_usd > 0.0);
        assert_eq!(summary.usage_by_provider.get("openai"), Some(&1));
        assert_eq!(summary.usage_by_provider.get("anthropic"), Some(&1));
    }

    #[tokio::test]
    async fn test_pool_concurrency_limiting() {
        let pool = ProviderPool::new(2).unwrap();
        let p1 = pool.acquire_permit().await.unwrap();
        let p2 = pool.acquire_permit().await.unwrap();
        assert_eq!(pool.concurrency_limit.available_permits(), 0);

        drop(p1);
        assert_eq!(pool.concurrency_limit.available_permits(), 1);
        drop(p2);
        assert_eq!(pool.concurrency_limit.available_permits(), 2);
    }
}
