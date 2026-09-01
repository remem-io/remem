//! Observability, performance metrics, and telemetry for the remem memory engine.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Snapshot of performance and operation metrics.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct MetricsSnapshot {
    pub total_stores: u64,
    pub total_recalls: u64,
    pub total_consolidations: u64,
    pub store_latency_p50_ms: f64,
    pub store_latency_p95_ms: f64,
    pub recall_latency_p50_ms: f64,
    pub recall_latency_p95_ms: f64,
    pub recall_latency_p99_ms: f64,
    pub active_sessions: usize,
    pub uptime_seconds: u64,
}

/// In-memory rolling latency and operation metrics tracker.
pub struct MemoryMetrics {
    start_time: Instant,
    total_stores: AtomicU64,
    total_recalls: AtomicU64,
    total_consolidations: AtomicU64,
    store_latencies: RwLock<VecDeque<f64>>,
    recall_latencies: RwLock<VecDeque<f64>>,
    max_samples: usize,
}

/// Optional supplemental metrics for Prometheus export.
#[derive(Debug, Clone, Default)]
pub struct PrometheusExtraMetrics {
    pub total_memories: Option<usize>,
    pub total_tokens: Option<u64>,
    pub total_cost_usd: Option<f64>,
    pub cache_hits: Option<u64>,
    pub cache_misses: Option<u64>,
}

impl MemoryMetrics {
    pub fn new(max_samples: usize) -> Self {
        Self {
            start_time: Instant::now(),
            total_stores: AtomicU64::new(0),
            total_recalls: AtomicU64::new(0),
            total_consolidations: AtomicU64::new(0),
            store_latencies: RwLock::new(VecDeque::with_capacity(max_samples)),
            recall_latencies: RwLock::new(VecDeque::with_capacity(max_samples)),
            max_samples,
        }
    }

    pub fn standard() -> Self {
        Self::new(1000)
    }

    /// Record a memory store operation latency.
    pub fn record_store(&self, duration: Duration) {
        self.total_stores.fetch_add(1, Ordering::Relaxed);
        let ms = duration.as_secs_f64() * 1000.0;
        if let Ok(mut lock) = self.store_latencies.write() {
            if lock.len() >= self.max_samples {
                lock.pop_front();
            }
            lock.push_back(ms);
        }
    }

    /// Record a memory recall operation latency.
    pub fn record_recall(&self, duration: Duration) {
        self.total_recalls.fetch_add(1, Ordering::Relaxed);
        let ms = duration.as_secs_f64() * 1000.0;
        if let Ok(mut lock) = self.recall_latencies.write() {
            if lock.len() >= self.max_samples {
                lock.pop_front();
            }
            lock.push_back(ms);
        }
    }

    /// Record a consolidation run.
    pub fn record_consolidation(&self) {
        self.total_consolidations.fetch_add(1, Ordering::Relaxed);
    }

    /// Compute current metrics snapshot including latency percentiles.
    pub fn snapshot(&self, active_sessions: usize) -> MetricsSnapshot {
        let (store_p50, store_p95) = self.calculate_percentiles(&self.store_latencies);
        let (recall_p50, recall_p95, recall_p99) = self.calculate_recall_percentiles();

        MetricsSnapshot {
            total_stores: self.total_stores.load(Ordering::Relaxed),
            total_recalls: self.total_recalls.load(Ordering::Relaxed),
            total_consolidations: self.total_consolidations.load(Ordering::Relaxed),
            store_latency_p50_ms: store_p50,
            store_latency_p95_ms: store_p95,
            recall_latency_p50_ms: recall_p50,
            recall_latency_p95_ms: recall_p95,
            recall_latency_p99_ms: recall_p99,
            active_sessions,
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }

    fn calculate_percentiles(&self, lock: &RwLock<VecDeque<f64>>) -> (f64, f64) {
        let mut samples: Vec<f64> = lock
            .read()
            .map(|d| d.iter().copied().collect())
            .unwrap_or_default();
        if samples.is_empty() {
            return (0.0, 0.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50_idx = (samples.len() as f64 * 0.50) as usize;
        let p95_idx = ((samples.len() as f64 * 0.95) as usize).min(samples.len() - 1);
        (samples[p50_idx], samples[p95_idx])
    }

    fn calculate_recall_percentiles(&self) -> (f64, f64, f64) {
        let mut samples: Vec<f64> = self
            .recall_latencies
            .read()
            .map(|d| d.iter().copied().collect())
            .unwrap_or_default();
        if samples.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p50_idx = (samples.len() as f64 * 0.50) as usize;
        let p95_idx = ((samples.len() as f64 * 0.95) as usize).min(samples.len() - 1);
        let p99_idx = ((samples.len() as f64 * 0.99) as usize).min(samples.len() - 1);
        (samples[p50_idx], samples[p95_idx], samples[p99_idx])
    }

    /// Render Prometheus text exposition format metrics.
    pub fn render_prometheus(&self, snapshot: &MetricsSnapshot) -> String {
        self.render_prometheus_full(snapshot, None)
    }

    /// Render comprehensive Prometheus text exposition format metrics with store, cost, and cache data.
    pub fn render_prometheus_full(
        &self,
        snapshot: &MetricsSnapshot,
        extra: Option<&PrometheusExtraMetrics>,
    ) -> String {
        let mut out = format!(
            "# HELP remem_stores_total Total memories stored\n\
             # TYPE remem_stores_total counter\n\
             remem_stores_total {}\n\n\
             # HELP remem_recalls_total Total memory recall queries\n\
             # TYPE remem_recalls_total counter\n\
             remem_recalls_total {}\n\n\
             # HELP remem_consolidations_total Total consolidation passes executed\n\
             # TYPE remem_consolidations_total counter\n\
             remem_consolidations_total {}\n\n\
             # HELP remem_store_latency_p50_ms 50th percentile store latency in ms\n\
             # TYPE remem_store_latency_p50_ms gauge\n\
             remem_store_latency_p50_ms {:.2}\n\n\
             # HELP remem_store_latency_p95_ms 95th percentile store latency in ms\n\
             # TYPE remem_store_latency_p95_ms gauge\n\
             remem_store_latency_p95_ms {:.2}\n\n\
             # HELP remem_recall_latency_p50_ms 50th percentile recall latency in ms\n\
             # TYPE remem_recall_latency_p50_ms gauge\n\
             remem_recall_latency_p50_ms {:.2}\n\n\
             # HELP remem_recall_latency_p95_ms 95th percentile recall latency in ms\n\
             # TYPE remem_recall_latency_p95_ms gauge\n\
             remem_recall_latency_p95_ms {:.2}\n\n\
             # HELP remem_recall_latency_p99_ms 99th percentile recall latency in ms\n\
             # TYPE remem_recall_latency_p99_ms gauge\n\
             remem_recall_latency_p99_ms {:.2}\n\n\
             # HELP remem_active_sessions Number of active sessions\n\
             # TYPE remem_active_sessions gauge\n\
             remem_active_sessions {}\n\n\
             # HELP remem_uptime_seconds Process uptime in seconds\n\
             # TYPE remem_uptime_seconds counter\n\
             remem_uptime_seconds {}\n",
            snapshot.total_stores,
            snapshot.total_recalls,
            snapshot.total_consolidations,
            snapshot.store_latency_p50_ms,
            snapshot.store_latency_p95_ms,
            snapshot.recall_latency_p50_ms,
            snapshot.recall_latency_p95_ms,
            snapshot.recall_latency_p99_ms,
            snapshot.active_sessions,
            snapshot.uptime_seconds
        );

        if let Some(ext) = extra {
            if let Some(mem_count) = ext.total_memories {
                out.push_str(&format!(
                    "\n# HELP remem_active_memories_total Total active memories in database\n\
                     # TYPE remem_active_memories_total gauge\n\
                     remem_active_memories_total {}\n",
                    mem_count
                ));
            }

            if let Some(tokens) = ext.total_tokens {
                out.push_str(&format!(
                    "\n# HELP remem_llm_tokens_total Total LLM tokens consumed\n\
                     # TYPE remem_llm_tokens_total counter\n\
                     remem_llm_tokens_total {}\n",
                    tokens
                ));
            }

            if let Some(cost) = ext.total_cost_usd {
                out.push_str(&format!(
                    "\n# HELP remem_llm_cost_usd_total Estimated total LLM cost in USD\n\
                     # TYPE remem_llm_cost_usd_total counter\n\
                     remem_llm_cost_usd_total {:.6}\n",
                    cost
                ));
            }

            if let (Some(hits), Some(misses)) = (ext.cache_hits, ext.cache_misses) {
                out.push_str(&format!(
                    "\n# HELP remem_cache_hits_total Total embedding cache hits\n\
                     # TYPE remem_cache_hits_total counter\n\
                     remem_cache_hits_total {}\n\n\
                     # HELP remem_cache_misses_total Total embedding cache misses\n\
                     # TYPE remem_cache_misses_total counter\n\
                     remem_cache_misses_total {}\n",
                    hits, misses
                ));
            }
        }

        out
    }

    /// Calculate percentage of recall operations completing within target SLA (e.g. 50ms).
    pub fn calculate_sla_adherence(&self, target_max_ms: f64) -> f64 {
        let samples: Vec<f64> = self
            .recall_latencies
            .read()
            .map(|d| d.iter().copied().collect())
            .unwrap_or_default();
        if samples.is_empty() {
            return 100.0;
        }
        let within_sla = samples.iter().filter(|&&ms| ms <= target_max_ms).count();
        (within_sla as f64 / samples.len() as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording_and_percentiles() {
        let metrics = MemoryMetrics::new(100);
        for i in 1..=100 {
            metrics.record_recall(Duration::from_millis(i));
        }

        let snap = metrics.snapshot(3);
        assert_eq!(snap.total_recalls, 100);
        assert!(snap.recall_latency_p50_ms >= 45.0 && snap.recall_latency_p50_ms <= 55.0);
        assert!(snap.recall_latency_p95_ms >= 90.0);
        assert_eq!(snap.active_sessions, 3);
    }
}
