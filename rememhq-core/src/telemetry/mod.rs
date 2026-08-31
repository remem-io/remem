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
