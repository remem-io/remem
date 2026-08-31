//! Thread-safe in-memory embedding cache with TTL expiration and metrics.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct CacheEntry {
    vector: Vec<f32>,
    created_at: Instant,
}

/// Statistics for embedding cache usage.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_entries: usize,
    pub hit_rate_percentage: f64,
}

/// Thread-safe in-memory cache for embedding vectors with TTL expiration.
pub struct EmbeddingCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    ttl: Duration,
    max_capacity: usize,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl EmbeddingCache {
    /// Create a new embedding cache with the specified TTL and maximum capacity.
    pub fn new(ttl: Duration, max_capacity: usize) -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(max_capacity.min(1024))),
            ttl,
            max_capacity,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Create a cache with standard production defaults (1 hour TTL, 10,000 max entries).
    pub fn standard() -> Self {
        Self::new(Duration::from_secs(3600), 10_000)
    }

    /// Retrieve an embedding from cache if present and unexpired.
    pub fn get(&self, text: &str) -> Option<Vec<f32>> {
        let key = text.trim();
        let read = self.entries.read().ok()?;
        if let Some(entry) = read.get(key) {
            if entry.created_at.elapsed() <= self.ttl {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.vector.clone());
            }
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Insert or update an embedding vector in the cache.
    pub fn insert(&self, text: impl Into<String>, vector: Vec<f32>) {
        let key = text.into().trim().to_string();
        if let Ok(mut write) = self.entries.write() {
            if write.len() >= self.max_capacity && !write.contains_key(&key) {
                // Prune expired entries first
                let ttl = self.ttl;
                write.retain(|_, v| v.created_at.elapsed() <= ttl);

                // If still at capacity, evict oldest 10%
                if write.len() >= self.max_capacity {
                    let mut sorted_keys: Vec<(String, Instant)> = write
                        .iter()
                        .map(|(k, v)| (k.clone(), v.created_at))
                        .collect();
                    sorted_keys.sort_by_key(|(_, t)| *t);
                    let to_remove = (self.max_capacity / 10).max(1);
                    for (k, _) in sorted_keys.into_iter().take(to_remove) {
                        write.remove(&k);
                    }
                }
            }

            write.insert(
                key,
                CacheEntry {
                    vector,
                    created_at: Instant::now(),
                },
            );
        }
    }

    /// Clear all cached embeddings.
    pub fn clear(&self) {
        if let Ok(mut write) = self.entries.write() {
            write.clear();
        }
    }

    /// Get current cache statistics.
    pub fn stats(&self) -> CacheStats {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_entries = self.entries.read().map(|e| e.len()).unwrap_or(0);
        let total_requests = hits + misses;
        let hit_rate_percentage = if total_requests > 0 {
            (hits as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        CacheStats {
            hits,
            misses,
            total_entries,
            hit_rate_percentage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_get() {
        let cache = EmbeddingCache::new(Duration::from_secs(60), 100);
        assert!(cache.get("test text").is_none());

        cache.insert("test text", vec![0.1, 0.2, 0.3]);
        let hit = cache.get("test text").expect("Expected cache hit");
        assert_eq!(hit, vec![0.1, 0.2, 0.3]);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.total_entries, 1);
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let cache = EmbeddingCache::new(Duration::from_millis(10), 100);
        cache.insert("quick expire", vec![1.0, 2.0]);
        assert!(cache.get("quick expire").is_some());

        std::thread::sleep(Duration::from_millis(20));
        assert!(cache.get("quick expire").is_none());
    }

    #[test]
    fn test_cache_capacity_eviction() {
        let cache = EmbeddingCache::new(Duration::from_secs(60), 3);
        cache.insert("k1", vec![1.0]);
        cache.insert("k2", vec![2.0]);
        cache.insert("k3", vec![3.0]);
        assert_eq!(cache.stats().total_entries, 3);

        cache.insert("k4", vec![4.0]);
        // Capacity enforced
        assert!(cache.stats().total_entries <= 3);
    }
}
