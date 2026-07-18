//! Guided retrieval — HNSW search → LLM re-ranking → top-k with reasoning.
//!
//! This is the key operation that differentiates remem from naive vector stores.
//! Instead of returning raw cosine similarity results, the LLM reasons about
//! which memories are actually relevant to the query and explains why.

use crate::memory::types::{MemoryResult, MemoryType};
use crate::providers::{EmbeddingProvider, Provider, ProviderOptions};
use crate::storage::sqlite::SqliteStore;
use crate::storage::vector::VectorIndex;
use crate::storage::MemoryStore;
use chrono::{DateTime, Utc};

/// Perform guided retrieval: vector search → LLM re-ranking → reasoning traces.
#[allow(clippy::too_many_arguments)]
pub async fn guided_retrieval(
    provider: &dyn Provider,
    embeddings: &dyn EmbeddingProvider,
    store: &SqliteStore,
    index: &dyn VectorIndex,
    query: &str,
    limit: usize,
    filter_tags: &[String],
    since: Option<DateTime<Utc>>,
    memory_type: Option<MemoryType>,
    model: &str,
    options: Option<&ProviderOptions>,
) -> anyhow::Result<Vec<MemoryResult>> {
    // Step 1: Embed the query
    let query_embedding = embeddings.embed(query, options).await?;

    // Step 2: Get top-50 candidates from vector index
    let candidate_count = 50.min(index.len());
    if candidate_count == 0 {
        return Ok(Vec::new());
    }

    let vector_results = index.search(&query_embedding, candidate_count).await?;

    // Step 3: Fetch full records for candidates, applying filters
    let mut candidates: Vec<(MemoryResult, f32)> = Vec::new();

    for vr in &vector_results {
        if let Ok(Some(record)) = store.get(vr.id).await {
            // Apply filters
            if let Some(mt) = memory_type {
                if record.memory_type != mt {
                    continue;
                }
            }
            if let Some(since_dt) = since {
                if record.created_at < since_dt {
                    continue;
                }
            }
            if !filter_tags.is_empty() && !filter_tags.iter().any(|t| record.tags.contains(t)) {
                continue;
            }

            let mut result = MemoryResult::from(record);
            result.similarity = vr.similarity;
            candidates.push((result, vr.similarity));
        }
    }

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    // Step 4: LLM re-ranking with reasoning
    let reranked = llm_rerank(provider, query, &candidates, limit, model, options).await?;

    Ok(reranked)
}

/// Use the LLM to re-rank candidate memories and provide reasoning.
#[allow(clippy::too_many_arguments)]
async fn llm_rerank(
    provider: &dyn Provider,
    query: &str,
    candidates: &[(MemoryResult, f32)],
    limit: usize,
    model: &str,
    options: Option<&ProviderOptions>,
) -> anyhow::Result<Vec<MemoryResult>> {
    // Build the candidate list for the prompt
    let candidate_list: String = candidates
        .iter()
        .enumerate()
        .map(|(i, (result, sim))| {
            format!(
                "[{}] (similarity: {:.3}, importance: {}) {}",
                i + 1,
                sim,
                result.importance,
                result.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        r#"You are a memory retrieval assistant. Given a query and a list of candidate memories, select the {limit} most relevant memories and explain why each is relevant.

Query: "{query}"

Candidate memories:
{candidate_list}

For each selected memory, output a line in this exact format:
SELECTED [number] | [brief reasoning why this is relevant]

Select at most {limit} memories. Only select memories that are genuinely relevant to the query. Output nothing else."#
    );

    let (response, _usage) = provider.complete(&prompt, model, options).await?;

    // Parse the LLM response
    let mut results = Vec::new();
    let mut seen_indices = std::collections::HashSet::new();
    for line in response.lines() {
        let line = line.trim();
        if !line.starts_with("SELECTED") {
            continue;
        }

        // Parse "SELECTED [N] | [reasoning]"
        if let Some(rest) = line.strip_prefix("SELECTED") {
            let rest = rest.trim();
            let parts: Vec<&str> = rest.splitn(2, '|').collect();
            if parts.len() == 2 {
                let idx_str = parts[0].trim().trim_matches(|c: char| !c.is_ascii_digit());
                let reasoning = parts[1].trim().to_string();

                if let Ok(idx) = idx_str.parse::<usize>() {
                    // Guard against the LLM selecting the same candidate twice
                    // (e.g. re-listing it under a different rationale), which
                    // would otherwise let one memory occupy multiple slots
                    // within `limit` and crowd out a genuinely distinct result.
                    if idx >= 1 && idx <= candidates.len() && seen_indices.insert(idx) {
                        let mut result = candidates[idx - 1].0.clone();
                        result.reasoning = Some(reasoning);
                        results.push(result);
                    }
                }
            }
        }
    }

    // If LLM parsing failed, fall back to similarity-based ordering
    if results.is_empty() {
        tracing::warn!("LLM re-ranking produced no results, falling back to similarity ordering");
        results = candidates
            .iter()
            .take(limit)
            .map(|(r, _)| r.clone())
            .collect();
    }

    results.truncate(limit);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::MemoryType;
    use async_trait::async_trait;
    use uuid::Uuid;

    struct MockProviderObj {
        response: String,
    }

    #[async_trait]
    impl Provider for MockProviderObj {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
            Ok((
                self.response.clone(),
                Some(crate::providers::TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                }),
            ))
        }
        async fn chat(
            &self,
            _messages: &[crate::providers::ChatMessage],
            _tools: &[crate::providers::Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<crate::providers::ChatResponse> {
            unimplemented!()
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_llm_rerank_parsing() {
        let provider = MockProviderObj {
            response: "SELECTED 2 | Because it matches exactly\nSELECTED 1 | Good context"
                .to_string(),
        };

        let mem1 = MemoryResult {
            id: Uuid::new_v4(),
            content: "First memory".to_string(),
            tags: vec![],
            importance: 0.5,
            memory_type: MemoryType::Fact,
            created_at: Utc::now(),
            source_session: None,
            similarity: 0.8,
            decay_score: 1.0,
            reasoning: None,
        };

        let mem2 = MemoryResult {
            id: Uuid::new_v4(),
            content: "Second memory".to_string(),
            tags: vec![],
            importance: 0.9,
            memory_type: MemoryType::Fact,
            created_at: Utc::now(),
            source_session: None,
            similarity: 0.9,
            decay_score: 1.0,
            reasoning: None,
        };

        let candidates = vec![(mem1.clone(), 0.8), (mem2.clone(), 0.9)];

        let results = llm_rerank(&provider, "query", &candidates, 2, "mock", None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        // The LLM selected 2 then 1
        assert_eq!(results[0].id, mem2.id);
        assert_eq!(
            results[0].reasoning.as_deref(),
            Some("Because it matches exactly")
        );
        assert_eq!(results[1].id, mem1.id);
        assert_eq!(results[1].reasoning.as_deref(), Some("Good context"));
    }

    #[tokio::test]
    async fn test_llm_rerank_fallback() {
        // If LLM returns garbage, it should fallback to similarity ordering
        let provider = MockProviderObj {
            response: "No matches found".to_string(),
        };

        let mem1 = MemoryResult {
            id: Uuid::new_v4(),
            content: "First".to_string(),
            tags: vec![],
            importance: 0.5,
            memory_type: MemoryType::Fact,
            created_at: Utc::now(),
            source_session: None,
            similarity: 0.8,
            decay_score: 1.0,
            reasoning: None,
        };

        let candidates = vec![(mem1.clone(), 0.8)];

        let results = llm_rerank(&provider, "query", &candidates, 1, "mock", None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, mem1.id);
        assert_eq!(results[0].reasoning, None);
    }

    #[tokio::test]
    async fn test_llm_rerank_dedupes_repeated_selection() {
        // Regression test: if the LLM selects the same candidate index twice
        // (e.g. re-listing it under a different rationale), that memory used
        // to occupy two slots in `results`, silently crowding out a distinct
        // candidate within `limit`.
        let provider = MockProviderObj {
            response: "SELECTED 1 | First rationale\nSELECTED 1 | Duplicate rationale\nSELECTED 2 | Second memory"
                .to_string(),
        };

        let mem1 = MemoryResult {
            id: Uuid::new_v4(),
            content: "First".to_string(),
            tags: vec![],
            importance: 0.5,
            memory_type: MemoryType::Fact,
            created_at: Utc::now(),
            source_session: None,
            similarity: 0.8,
            decay_score: 1.0,
            reasoning: None,
        };
        let mem2 = MemoryResult {
            id: Uuid::new_v4(),
            content: "Second".to_string(),
            tags: vec![],
            importance: 0.5,
            memory_type: MemoryType::Fact,
            created_at: Utc::now(),
            source_session: None,
            similarity: 0.7,
            decay_score: 1.0,
            reasoning: None,
        };

        let candidates = vec![(mem1.clone(), 0.8), (mem2.clone(), 0.7)];

        let results = llm_rerank(&provider, "query", &candidates, 5, "mock", None)
            .await
            .unwrap();

        assert_eq!(results.len(), 2, "duplicate selection must not produce duplicate results");
        assert_eq!(results[0].id, mem1.id);
        assert_eq!(results[0].reasoning.as_deref(), Some("First rationale"));
        assert_eq!(results[1].id, mem2.id);
    }
}
