//! Streaming Consolidation Pipeline for Continuous High-Volume Sessions.
//!
//! Processes active observation logs in 100-observation batches, piping extraction,
//! asynchronous vector embedding, and incremental deduplication with checkpoint recovery.

use crate::memory::types::{ConsolidationReport, MemoryRecord};
use crate::providers::{EmbeddingProvider, Provider, ProviderOptions};
use crate::reasoning::checkpoint::{CheckpointManager, CheckpointStatus, ConsolidationCheckpoint};
use crate::reasoning::consolidation::{extract_facts, ExtractedFact};
use crate::storage::sqlite::SqliteStore;
use crate::storage::vector::VectorIndex;
use crate::storage::MemoryStore;
use chrono::Utc;
use std::sync::Arc;

/// Chunked streaming consolidator for large continuous workloads.
pub struct StreamingConsolidationPipeline {
    pub chunk_size: usize,
    pub checkpoint_manager: Option<Arc<CheckpointManager>>,
}

impl StreamingConsolidationPipeline {
    pub fn new(chunk_size: usize, checkpoint_manager: Option<Arc<CheckpointManager>>) -> Self {
        Self {
            chunk_size: chunk_size.max(10),
            checkpoint_manager,
        }
    }

    pub fn default_100() -> Self {
        Self::new(100, None)
    }

    /// Run streaming consolidation over a sequence of session memories with checkpointing.
    #[allow(clippy::too_many_arguments)]
    pub async fn process_stream(
        &self,
        provider: &dyn Provider,
        embeddings: &dyn EmbeddingProvider,
        store: &SqliteStore,
        index: &dyn VectorIndex,
        session_id: &str,
        memories: &[MemoryRecord],
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<ConsolidationReport> {
        let total_observations = memories.len();
        if total_observations == 0 {
            return Ok(ConsolidationReport {
                session_id: session_id.to_string(),
                new_facts: 0,
                updated_facts: 0,
                contradictions: Vec::new(),
                knowledge_graph_updates: Vec::new(),
            });
        }

        let mut start_idx = 0;
        let mut extracted_facts: Vec<ExtractedFact> = Vec::new();

        // Check for resumable checkpoint
        if let Some(cp_mgr) = &self.checkpoint_manager {
            if let Ok(Some(cp)) = cp_mgr.load_checkpoint(session_id) {
                if cp.status == CheckpointStatus::InProgress
                    && cp.processed_observations < total_observations
                {
                    tracing::info!(
                        session_id = session_id,
                        resuming_from = cp.processed_observations,
                        "Resuming consolidation pipeline from checkpoint"
                    );
                    start_idx = cp.processed_observations;
                }
            }
        }

        let chunks: Vec<&[MemoryRecord]> = memories[start_idx..].chunks(self.chunk_size).collect();
        let total_steps = chunks.len();

        for (step_idx, chunk) in chunks.iter().enumerate() {
            let session_content = chunk
                .iter()
                .map(|m| format!("- [{}] {}", m.memory_type, m.content))
                .collect::<Vec<_>>()
                .join("\n");

            let mut batch_facts = extract_facts(provider, &session_content, model, options).await?;
            extracted_facts.append(&mut batch_facts);

            // Update checkpoint after each chunk
            if let Some(cp_mgr) = &self.checkpoint_manager {
                let cp = ConsolidationCheckpoint {
                    session_id: session_id.to_string(),
                    step_index: step_idx + 1,
                    total_steps,
                    processed_observations: start_idx + (step_idx + 1) * self.chunk_size,
                    extracted_facts_count: extracted_facts.len(),
                    last_updated: Utc::now(),
                    status: CheckpointStatus::InProgress,
                    intermediate_facts: extracted_facts.iter().map(|f| f.content.clone()).collect(),
                };
                let _ = cp_mgr.save_checkpoint(&cp);
            }
        }

        // Background embedding generation & incremental deduplication
        let mut inserts = Vec::new();
        let mut updates = Vec::new();
        let mut triples = Vec::new();
        let mut index_adds = Vec::new();
        let mut new_count = 0;
        let mut updated_count = 0;
        let mut kg_updates = Vec::new();

        let embed_futures: Vec<_> = extracted_facts
            .iter()
            .map(|f| embeddings.embed(&f.content, options))
            .collect();
        let embedding_results = futures_util::future::join_all(embed_futures).await;

        for (fact, emb_res) in extracted_facts.iter().zip(embedding_results) {
            let embedding = emb_res?;
            let mut record = MemoryRecord::new(&fact.content, fact.memory_type)
                .with_importance(fact.importance)
                .with_tags(fact.tags.clone())
                .with_session(session_id);
            record.embedding = Some(embedding.clone());

            // Incremental dedup check against vector index
            let existing_matches = index.search(&embedding, 2).await?;
            let mut is_update = false;

            for em in existing_matches {
                if em.similarity > 0.94 {
                    if let Ok(Some(existing)) = store.get(em.id).await {
                        let mut updated = existing;
                        updated.content = record.content.clone();
                        updated.importance = record.importance.max(updated.importance);
                        updated.updated_at = Utc::now();
                        updates.push(updated.clone());
                        index_adds.push((updated.id, embedding.clone()));
                        updated_count += 1;
                        is_update = true;
                        break;
                    }
                }
            }

            if !is_update {
                inserts.push(record.clone());
                index_adds.push((record.id, embedding.clone()));
                new_count += 1;
            }

            if let Some(t) = &fact.knowledge_triple {
                kg_updates.push(t.clone());
                triples.push((t.clone(), record.id));
            }
        }

        // Commit transaction to SQLite
        store
            .save_consolidation(&inserts, &updates, &[], &triples)
            .await?;

        // Add to vector index
        for (id, emb) in index_adds {
            let _ = index.add(id, &emb).await;
        }

        // Mark checkpoint completed and clear
        if let Some(cp_mgr) = &self.checkpoint_manager {
            let _ = cp_mgr.clear_checkpoint(session_id);
        }

        Ok(ConsolidationReport {
            session_id: session_id.to_string(),
            new_facts: new_count,
            updated_facts: updated_count,
            contradictions: Vec::new(),
            knowledge_graph_updates: kg_updates,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::MemoryType;
    use crate::providers::mock::{MockEmbeddings, MockProvider};
    use crate::storage::vector::HNSWVectorIndex;

    #[tokio::test]
    async fn test_streaming_pipeline_execution() {
        let store = SqliteStore::open_in_memory().unwrap();
        let index = HNSWVectorIndex::new(4, 100);
        let mock_provider = MockProvider;
        let mock_embeddings = MockEmbeddings::new(4);

        let pipeline = StreamingConsolidationPipeline::new(10, None);
        let memories = vec![
            MemoryRecord::new(
                "User discussed Rust compiler semantics",
                MemoryType::Observation,
            ),
            MemoryRecord::new(
                "User asked about lifetimes in Rust",
                MemoryType::Observation,
            ),
        ];

        let report = pipeline
            .process_stream(
                &mock_provider,
                &mock_embeddings,
                &store,
                &index,
                "sess-test",
                &memories,
                "mock-model",
                None,
            )
            .await
            .unwrap();

        assert_eq!(report.new_facts, 1);
        assert_eq!(report.knowledge_graph_updates.len(), 1);
    }
}
