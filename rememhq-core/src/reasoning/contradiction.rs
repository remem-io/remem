use crate::memory::types::Contradiction;
use crate::providers::{EmbeddingProvider, Provider};
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
) -> anyhow::Result<Vec<Contradiction>> {
    if new_facts.is_empty() {
        return Ok(Vec::new());
    }

    let mut contradictions = Vec::new();

    for fact in new_facts.iter() {
        // Find top-5 potential conflicts using vector similarity
        let embedding = embeddings.embed(&fact.content).await?;
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

        let response = provider.complete(&prompt, model).await?;

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
