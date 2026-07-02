use crate::memory::types::Contradiction;
use crate::providers::{EmbeddingProvider, Provider, ProviderOptions};
use crate::storage::vector::VectorIndex;
use crate::storage::MemoryStore;

/// Detect contradictions between new facts and existing memories.
/// Uses the vector index to find potentially conflicting candidates first.
pub(crate) async fn detect_contradictions(
    provider: &dyn Provider,
    embeddings: &dyn EmbeddingProvider,
    index: &dyn VectorIndex,
    store: &dyn MemoryStore,
    new_facts: &[super::consolidation::ExtractedFact],
    model: &str,
    options: Option<&ProviderOptions>,
) -> anyhow::Result<Vec<Contradiction>> {
    if new_facts.is_empty() {
        return Ok(Vec::new());
    }

    let mut contradictions = Vec::new();

    for fact in new_facts {
        // Find top-5 potential conflicts using vector similarity
        let embedding = embeddings.embed(&fact.content, options).await?;
        let results = index.search(&embedding, 5).await?;

        if results.is_empty() {
            continue;
        }

        // Fetch actual memories for these candidates
        let mut candidates = Vec::new();
        for res in results {
            if let Ok(Some(m)) = store.get(res.id).await {
                // Skip if it's the exact same content (those are handled by update logic)
                if m.content.trim() == fact.content.trim() {
                    continue;
                }
                candidates.push(m);
            }
        }

        if candidates.is_empty() {
            continue;
        }

        // Build targeted prompt for this specific fact
        let candidates_text: String = candidates
            .iter()
            .enumerate()
            .map(|(j, m)| format!("[CANDIDATE-{}] {}", j + 1, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"You are a contradiction detector. Compare the NEW FACT below against the EXISTING CANDIDATES and identify if any directly contradict it.

NEW FACT:
{}

EXISTING CANDIDATES:
{}

If a contradiction is found, explain WHY they conflict.
Format: CONTRADICTION | [CANDIDATE-N] | [explanation]
If no contradiction exists, output: NONE"#,
            fact.content, candidates_text
        );

        let (response, _usage) = provider.complete(&prompt, model, options).await?;

        for line in response.lines() {
            let line = line.trim();
            if line.starts_with("CONTRADICTION |") {
                let parts: Vec<&str> = line.splitn(3, '|').collect();
                if parts.len() == 3 {
                    let cand_idx_str = parts[1]
                        .trim()
                        .trim_start_matches("[CANDIDATE-")
                        .trim_end_matches(']');
                    if let Ok(cand_idx) = cand_idx_str.parse::<usize>() {
                        if cand_idx >= 1 && cand_idx <= candidates.len() {
                            let existing = &candidates[cand_idx - 1];
                            contradictions.push(Contradiction {
                                existing_memory_id: existing.id,
                                new_content: fact.content.clone(),
                                existing_content: existing.content.clone(),
                                explanation: parts[2].trim().to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(contradictions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::SqliteStore;
    use crate::storage::vector::VectorResult;
    use async_trait::async_trait;
    use std::path::Path;
    use uuid::Uuid;

    struct MockEmbeddings;
    #[async_trait]
    impl EmbeddingProvider for MockEmbeddings {
        async fn embed(
            &self,
            _text: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.0; 768])
        }
        async fn embed_batch(
            &self,
            _texts: &[String],
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }
        fn dimension(&self) -> usize {
            768
        }
    }

    struct MockIndex;
    #[async_trait]
    impl VectorIndex for MockIndex {
        async fn add(&self, _id: Uuid, _embedding: &[f32]) -> anyhow::Result<()> {
            Ok(())
        }
        async fn remove(&self, _id: Uuid) -> anyhow::Result<()> {
            Ok(())
        }
        async fn search(&self, _query: &[f32], _k: usize) -> anyhow::Result<Vec<VectorResult>> {
            Ok(vec![])
        }
        fn len(&self) -> usize {
            0
        }
        async fn save(&self, _path: &Path) -> anyhow::Result<()> {
            Ok(())
        }
        async fn load(&self, _path: &Path) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct MockProviderObj;
    #[async_trait]
    impl Provider for MockProviderObj {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
            Ok((
                "NONE".to_string(),
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
    async fn test_empty_facts() {
        let provider = MockProviderObj;
        let embeddings = MockEmbeddings;
        let index = MockIndex;
        let store = SqliteStore::open_in_memory().unwrap();
        let result =
            detect_contradictions(&provider, &embeddings, &index, &store, &[], "mock", None)
                .await
                .unwrap();
        assert!(result.is_empty());
    }
}
