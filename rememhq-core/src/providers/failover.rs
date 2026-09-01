//! Multi-provider LLM failover, circuit breaking, key load balancing, and request deduplication.

use crate::providers::resiliency::CircuitBreaker;
use crate::providers::{
    ChatMessage, ChatResponse, EmbeddingProvider, Provider, ProviderOptions, TokenUsage, Tool,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health and usage metrics for a provider entry in a failover chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderMetrics {
    pub name: String,
    pub total_requests: u64,
    pub total_errors: u64,
    pub circuit_open_count: u64,
    pub last_error: Option<String>,
}

/// A node in the failover chain containing the provider and its circuit breaker.
pub struct ChainNode<T: ?Sized> {
    pub provider: Arc<T>,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub requests: AtomicU64,
    pub errors: AtomicU64,
    pub last_error: RwLock<Option<String>>,
}

impl<T: ?Sized> ChainNode<T> {
    pub fn new(provider: Arc<T>) -> Self {
        Self {
            provider,
            circuit_breaker: Arc::new(CircuitBreaker::standard()),
            requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            last_error: RwLock::new(None),
        }
    }
}

/// Rotator for load-balancing across multiple API keys for a provider.
#[derive(Debug, Default)]
pub struct KeyRotator {
    keys: Vec<String>,
    counter: AtomicUsize,
}

impl KeyRotator {
    pub fn new(keys: Vec<String>) -> Self {
        let trimmed: Vec<String> = keys
            .into_iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect();
        Self {
            keys: trimmed,
            counter: AtomicUsize::new(0),
        }
    }

    /// From comma-separated keys string (e.g. from environment variable).
    pub fn from_csv(csv: &str) -> Self {
        let keys = csv.split(',').map(|s| s.to_string()).collect();
        Self::new(keys)
    }

    /// Select next API key using round-robin.
    pub fn next_key(&self) -> Option<String> {
        if self.keys.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed);
        Some(self.keys[idx % self.keys.len()].clone())
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Multi-provider LLM failover orchestrator.
/// Automatically falls back through a sequence of LLM providers if the primary fails or trips its circuit breaker.
pub struct ProviderChain {
    nodes: Vec<ChainNode<dyn Provider>>,
    name: String,
}

impl ProviderChain {
    pub fn new(providers: Vec<Arc<dyn Provider>>) -> Self {
        let nodes = providers.into_iter().map(ChainNode::new).collect();
        Self {
            nodes,
            name: "provider-chain".to_string(),
        }
    }

    /// Get current health and error metrics for all providers in the chain.
    pub async fn metrics(&self) -> Vec<ProviderMetrics> {
        let mut result = Vec::new();
        for node in &self.nodes {
            let last_err = node.last_error.read().await.clone();
            result.push(ProviderMetrics {
                name: node.provider.name().to_string(),
                total_requests: node.requests.load(Ordering::Relaxed),
                total_errors: node.errors.load(Ordering::Relaxed),
                circuit_open_count: node.circuit_breaker.trips(),
                last_error: last_err,
            });
        }
        result
    }
}

#[async_trait]
impl Provider for ProviderChain {
    async fn complete(
        &self,
        prompt: &str,
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<(String, Option<TokenUsage>)> {
        let mut last_err = None;

        for node in &self.nodes {
            if !node.circuit_breaker.allow_request() {
                tracing::warn!(
                    "Provider '{}' circuit breaker is OPEN. Skipping to fallback.",
                    node.provider.name()
                );
                continue;
            }

            node.requests.fetch_add(1, Ordering::Relaxed);
            match node.provider.complete(prompt, model, options).await {
                Ok(res) => {
                    node.circuit_breaker.record_success();
                    return Ok(res);
                }
                Err(e) => {
                    node.errors.fetch_add(1, Ordering::Relaxed);
                    node.circuit_breaker.record_failure();
                    let err_msg = e.to_string();
                    *node.last_error.write().await = Some(err_msg.clone());
                    tracing::warn!(
                        "Provider '{}' complete() failed with error: {}. Trying fallback…",
                        node.provider.name(),
                        err_msg
                    );
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("All providers in chain failed or have open circuit breakers")
        }))
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[Tool],
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<ChatResponse> {
        let mut last_err = None;

        for node in &self.nodes {
            if !node.circuit_breaker.allow_request() {
                tracing::warn!(
                    "Provider '{}' circuit breaker is OPEN. Skipping to fallback.",
                    node.provider.name()
                );
                continue;
            }

            node.requests.fetch_add(1, Ordering::Relaxed);
            match node.provider.chat(messages, tools, model, options).await {
                Ok(res) => {
                    node.circuit_breaker.record_success();
                    return Ok(res);
                }
                Err(e) => {
                    node.errors.fetch_add(1, Ordering::Relaxed);
                    node.circuit_breaker.record_failure();
                    let err_msg = e.to_string();
                    *node.last_error.write().await = Some(err_msg.clone());
                    tracing::warn!(
                        "Provider '{}' chat() failed with error: {}. Trying fallback…",
                        node.provider.name(),
                        err_msg
                    );
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("All providers in chain failed or have open circuit breakers")
        }))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Multi-provider embedding failover orchestrator.
pub struct EmbeddingChain {
    nodes: Vec<ChainNode<dyn EmbeddingProvider>>,
}

impl EmbeddingChain {
    pub fn new(providers: Vec<Arc<dyn EmbeddingProvider>>) -> Self {
        let nodes = providers.into_iter().map(ChainNode::new).collect();
        Self { nodes }
    }
}

#[async_trait]
impl EmbeddingProvider for EmbeddingChain {
    async fn embed(
        &self,
        text: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<Vec<f32>> {
        let mut last_err = None;

        for node in &self.nodes {
            if !node.circuit_breaker.allow_request() {
                continue;
            }

            node.requests.fetch_add(1, Ordering::Relaxed);
            match node.provider.embed(text, options).await {
                Ok(res) => {
                    node.circuit_breaker.record_success();
                    return Ok(res);
                }
                Err(e) => {
                    node.errors.fetch_add(1, Ordering::Relaxed);
                    node.circuit_breaker.record_failure();
                    let err_msg = e.to_string();
                    *node.last_error.write().await = Some(err_msg.clone());
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("All embedding providers in chain failed or have open circuit breakers")
        }))
    }

    async fn embed_batch(
        &self,
        texts: &[String],
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut last_err = None;

        for node in &self.nodes {
            if !node.circuit_breaker.allow_request() {
                continue;
            }

            node.requests.fetch_add(1, Ordering::Relaxed);
            match node.provider.embed_batch(texts, options).await {
                Ok(res) => {
                    node.circuit_breaker.record_success();
                    return Ok(res);
                }
                Err(e) => {
                    node.errors.fetch_add(1, Ordering::Relaxed);
                    node.circuit_breaker.record_failure();
                    let err_msg = e.to_string();
                    *node.last_error.write().await = Some(err_msg.clone());
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            anyhow::anyhow!("All embedding providers in chain failed or have open circuit breakers")
        }))
    }

    fn dimension(&self) -> usize {
        self.nodes
            .first()
            .map(|n| n.provider.dimension())
            .unwrap_or(768)
    }
}

/// Request deduplicator to coalesce concurrent identical LLM calls.
#[derive(Default)]
pub struct RequestDeduplicator {
    in_flight: Arc<RwLock<HashMap<String, Arc<tokio::sync::broadcast::Sender<String>>>>>,
}

impl RequestDeduplicator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Deduplicate execution: if the same prompt key is currently executing, wait for its broadcast result.
    pub async fn execute_or_join<F, Fut>(&self, key: &str, fetch: F) -> anyhow::Result<String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<String>>,
    {
        let (tx, mut rx, is_leader) = {
            let mut map = self.in_flight.write().await;
            if let Some(existing) = map.get(key) {
                let rx = existing.subscribe();
                (None, rx, false)
            } else {
                let (tx, rx) = tokio::sync::broadcast::channel(1);
                let tx_arc = Arc::new(tx);
                map.insert(key.to_string(), tx_arc.clone());
                (Some(tx_arc), rx, true)
            }
        };

        if is_leader {
            let result = fetch().await;
            let mut map = self.in_flight.write().await;
            map.remove(key);

            match result {
                Ok(val) => {
                    if let Some(tx_arc) = tx {
                        let _ = tx_arc.send(val.clone());
                    }
                    Ok(val)
                }
                Err(e) => Err(e),
            }
        } else {
            rx.recv()
                .await
                .map_err(|e| anyhow::anyhow!("Deduplication channel error: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockProvider;

    struct FailingProvider {
        name: String,
    }

    #[async_trait]
    impl Provider for FailingProvider {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<TokenUsage>)> {
            anyhow::bail!("Provider {} is down", self.name)
        }

        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<ChatResponse> {
            anyhow::bail!("Provider {} is down", self.name)
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn test_provider_chain_failover() {
        let p1: Arc<dyn Provider> = Arc::new(FailingProvider {
            name: "primary-failing".into(),
        });
        let p2: Arc<dyn Provider> = Arc::new(MockProvider);

        let chain = ProviderChain::new(vec![p1, p2]);
        let (output, _) = chain.complete("Hello", "test-model", None).await.unwrap();
        assert!(!output.is_empty());

        let metrics = chain.metrics().await;
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].total_errors, 1);
        assert_eq!(metrics[1].total_requests, 1);
    }

    #[test]
    fn test_key_rotator() {
        let rotator = KeyRotator::from_csv("key1, key2, key3");
        assert_eq!(rotator.next_key(), Some("key1".to_string()));
        assert_eq!(rotator.next_key(), Some("key2".to_string()));
        assert_eq!(rotator.next_key(), Some("key3".to_string()));
        assert_eq!(rotator.next_key(), Some("key1".to_string()));
    }
}
