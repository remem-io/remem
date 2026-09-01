//! Query Expansion & Synonym Detection.
//!
//! Enhances semantic retrieval by generating domain synonyms, multi-lingual variants,
//! and tuning hybrid BM25 + dense vector Reciprocal Rank Fusion (RRF).

use crate::memory::types::MemoryResult;
use crate::providers::{Provider, ProviderOptions};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Query expansion configuration and in-memory LRU cache.
pub struct QueryExpander {
    cache: Arc<RwLock<HashMap<String, Vec<String>>>>,
    max_cache_size: usize,
}

impl Default for QueryExpander {
    fn default() -> Self {
        Self::new(256)
    }
}

impl QueryExpander {
    pub fn new(max_cache_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_size,
        }
    }

    /// Expand a query into alternative keywords, synonyms, and sub-queries using an LLM.
    pub async fn expand_query(
        &self,
        provider: &dyn Provider,
        query: &str,
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<Vec<String>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        // Check cache first
        {
            let cache_read = self.cache.read().await;
            if let Some(cached) = cache_read.get(trimmed) {
                return Ok(cached.clone());
            }
        }

        let prompt = format!(
            r#"Generate 3 to 5 high-precision search query variations and domain synonyms for the following query to improve semantic retrieval.
Include technical equivalents, abbreviations, and related terms.
Output each variation on a new line without numbers or bullets.

Query: "{}"

Variations:"#,
            trimmed
        );

        let (response, _usage) = provider.complete(&prompt, model, options).await?;
        let mut variations: Vec<String> = response
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches(|c: char| {
                        c.is_ascii_digit() || c == '.' || c == '-' || c == '*'
                    })
                    .trim()
                    .to_string()
            })
            .filter(|l| !l.is_empty() && l.to_lowercase() != trimmed.to_lowercase())
            .collect();

        variations.insert(0, trimmed.to_string());
        variations.truncate(5);

        // Store in cache
        {
            let mut cache_write = self.cache.write().await;
            if cache_write.len() >= self.max_cache_size {
                cache_write.clear();
            }
            cache_write.insert(trimmed.to_string(), variations.clone());
        }

        Ok(variations)
    }

    /// Reciprocal Rank Fusion (RRF) to merge and rank results from dense vector and BM25 full-text search.
    pub fn reciprocal_rank_fusion(
        vector_results: &[MemoryResult],
        fts_results: &[MemoryResult],
        k: f64,
        vector_weight: f64,
        fts_weight: f64,
    ) -> Vec<MemoryResult> {
        let mut scores: HashMap<uuid::Uuid, (f64, MemoryResult)> = HashMap::new();

        for (rank, res) in vector_results.iter().enumerate() {
            let score = vector_weight / (k + (rank + 1) as f64);
            let entry = scores.entry(res.id).or_insert((0.0, res.clone()));
            entry.0 += score;
        }

        for (rank, res) in fts_results.iter().enumerate() {
            let score = fts_weight / (k + (rank + 1) as f64);
            let entry = scores.entry(res.id).or_insert((0.0, res.clone()));
            entry.0 += score;
        }

        let mut ranked: Vec<(f64, MemoryResult)> = scores.into_values().collect();
        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        ranked
            .into_iter()
            .map(|(score, mut res)| {
                res.similarity = score.clamp(0.0, 1.0) as f32;
                res
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::{MemoryRecord, MemoryType};
    use crate::providers::mock::MockProvider;

    #[tokio::test]
    async fn test_query_expansion_cache() {
        let expander = QueryExpander::new(10);
        let mock = MockProvider;

        let variations = expander
            .expand_query(&mock, "Rust ownership", "mock-model", None)
            .await
            .unwrap();

        assert!(variations.contains(&"Rust ownership".to_string()));
        assert!(variations.contains(&"Rust memory safety".to_string()));

        // Second call should hit the cache without consuming mock responses
        let cached_variations = expander
            .expand_query(&mock, "Rust ownership", "mock-model", None)
            .await
            .unwrap();
        assert_eq!(variations, cached_variations);
    }

    #[test]
    fn test_reciprocal_rank_fusion() {
        let rec1 = MemoryRecord::new("Memory One", MemoryType::Fact);
        let rec2 = MemoryRecord::new("Memory Two", MemoryType::Fact);

        let vec_res = vec![
            MemoryResult::from(rec1.clone()),
            MemoryResult::from(rec2.clone()),
        ];
        let fts_res = vec![
            MemoryResult::from(rec2.clone()),
            MemoryResult::from(rec1.clone()),
        ];

        let fused = QueryExpander::reciprocal_rank_fusion(&vec_res, &fts_res, 60.0, 1.0, 1.0);
        assert_eq!(fused.len(), 2);
    }
}
