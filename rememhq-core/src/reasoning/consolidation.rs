//! Consolidation — extract durable facts from raw session interactions.
//!
//! When a session ends, the consolidation engine uses an LLM to:
//! 1. Extract durable facts from the raw interaction log
//! 2. Score each fact's importance
//! 3. Detect contradictions with existing memories
//! 4. Update the knowledge graph

use crate::memory::types::{ConsolidationReport, KnowledgeGraphUpdate, MemoryRecord, MemoryType};
use crate::providers::{EmbeddingProvider, Provider};
use crate::storage::sqlite::SqliteStore;
use crate::storage::vector::VectorIndex;
use crate::storage::MemoryStore;

/// Run a consolidation pass over a session's memories.
pub async fn consolidate_session(
    provider: &dyn Provider,
    embeddings: &dyn EmbeddingProvider,
    store: &SqliteStore,
    index: &dyn VectorIndex,
    session_id: &str,
    model: &str,
) -> anyhow::Result<ConsolidationReport> {
    // Get all memories from this session
    let session_memories = store
        .list(&[], None, None, 1000)
        .await?
        .into_iter()
        .filter(|m| m.source_session.as_deref() == Some(session_id))
        .collect::<Vec<_>>();

    if session_memories.is_empty() {
        return Ok(ConsolidationReport {
            session_id: session_id.to_string(),
            new_facts: 0,
            updated_facts: 0,
            contradictions: Vec::new(),
            knowledge_graph_updates: Vec::new(),
        });
    }

    // Build the session content for the LLM
    let session_content: String = session_memories
        .iter()
        .map(|m| format!("- [{}] {}", m.memory_type, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    // Step 1: Extract durable facts
    let mut facts = extract_facts(provider, &session_content, model).await?;

    // Step 1b: Resolve entities in Knowledge Graph triples
    let resolver = super::resolution::LlmEntityResolver::new(provider, model.to_string(), store);
    use super::resolution::EntityResolver;

    // Collect all triples from facts
    let mut triples = Vec::new();
    for f in &facts {
        if let Some(t) = &f.knowledge_triple {
            triples.push(t.clone());
        }
    }

    if !triples.is_empty() {
        let resolved_triples = resolver.resolve(triples).await?;
        // Map resolved triples back to facts
        let mut triple_idx = 0;
        for f in &mut facts {
            if f.knowledge_triple.is_some() {
                f.knowledge_triple = Some(resolved_triples[triple_idx].clone());
                triple_idx += 1;
            }
        }
    }

    // Step 2: Check for contradictions with existing memories
    let contradictions = super::contradiction::detect_contradictions(
        provider, embeddings, index, store, &facts, model,
    )
    .await?;

    // Auto-resolve contradictions by archiving the old superseded memories
    for c in &contradictions {
        tracing::info!(
            old_memory_id = %c.existing_memory_id,
            explanation = %c.explanation,
            "Auto-resolving contradiction by archiving superseded memory"
        );
        let _ = store.archive(c.existing_memory_id).await;
    }

    // Step 3: Store new facts
    let mut new_count = 0;
    let mut updated_count = 0;
    let mut kg_updates = Vec::new();

    for fact in &facts {
        let mut record = MemoryRecord::new(&fact.content, fact.memory_type)
            .with_importance(fact.importance)
            .with_tags(fact.tags.clone())
            .with_session(session_id);

        // Generate embedding
        let embedding = embeddings.embed(&record.content).await?;
        record.embedding = Some(embedding.clone());

        // Check if this fact updates an existing memory
        let existing_results = index.search(&embedding, 3).await?;
        let mut is_update = false;

        for er in &existing_results {
            if er.similarity > 0.92 {
                // Very similar — this is an update, not a new fact
                if let Ok(Some(existing)) = store.get(er.id).await {
                    let mut updated = existing;
                    updated.content = record.content.clone();
                    updated.importance = record.importance.max(updated.importance);
                    updated.updated_at = chrono::Utc::now();
                    store.update(&updated).await?;
                    index.add(updated.id, &embedding).await?;
                    updated_count += 1;
                    is_update = true;
                    break;
                }
            }
        }

        if !is_update {
            store.insert(&record).await?;
            index.add(record.id, &embedding).await?;
            new_count += 1;
        }

        // Extract knowledge graph triples
        if let Some(triple) = &fact.knowledge_triple {
            kg_updates.push(triple.clone());
            // Persist the triple to the SQLite knowledge_graph table
            let _ = store.insert_knowledge_triple(triple, record.id).await;
        }
    }

    Ok(ConsolidationReport {
        session_id: session_id.to_string(),
        new_facts: new_count,
        updated_facts: updated_count,
        contradictions,
        knowledge_graph_updates: kg_updates,
    })
}

/// A fact extracted by the LLM during consolidation.
#[derive(Debug)]
pub(crate) struct ExtractedFact {
    pub(crate) content: String,
    pub(crate) importance: f32,
    pub(crate) memory_type: MemoryType,
    pub(crate) tags: Vec<String>,
    pub(crate) knowledge_triple: Option<KnowledgeGraphUpdate>,
}

/// Use the LLM to extract durable facts from raw session content.
async fn extract_facts(
    provider: &dyn Provider,
    session_content: &str,
    model: &str,
) -> anyhow::Result<Vec<ExtractedFact>> {
    let prompt = format!(
        r#"You are a memory consolidation engine. Extract durable, reusable facts from this session log.

For each fact, output a line in this exact format:
FACT | [type: fact/procedure/preference/decision] | [importance: 1-10] | [tags: comma-separated] | [content]

Optionally, if the fact represents a relationship, add a knowledge triple:
TRIPLE | [subject] | [predicate] | [object]

Special Case: PROCEDURES
If you extract a procedure with multiple steps, output EACH STEP as a separate FACT with `type: procedure`.
Link them using knowledge triples with the predicate `next_step`.
Example:
FACT | procedure | 8 | deploy | To deploy, first run build
TRIPLE | To deploy, first run build | next_step | Then run push
FACT | procedure | 8 | deploy | Then run push

Session log:
{session_content}

Rules:
- Only extract information worth remembering long-term
- Merge redundant information into single facts
- Score importance based on how useful this would be in future sessions
- Use specific, actionable language
- Do NOT include ephemeral details (timestamps, temporary states)

Output the facts now:"#
    );

    let response = provider.complete(&prompt, model).await?;

    let mut facts = Vec::new();
    let mut current_triple: Option<KnowledgeGraphUpdate> = None;

    for line in response.lines() {
        let line = line.trim();

        if line.starts_with("TRIPLE |") {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() == 4 {
                current_triple = Some(KnowledgeGraphUpdate {
                    subject: parts[1].trim().to_string(),
                    predicate: parts[2].trim().to_string(),
                    object: parts[3].trim().to_string(),
                });
            }
            continue;
        }

        if !line.starts_with("FACT |") {
            continue;
        }

        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() < 5 {
            continue;
        }

        let memory_type = parts[1].trim().parse().unwrap_or(MemoryType::Fact);
        let importance = parts[2]
            .trim()
            .parse::<f32>()
            .unwrap_or(5.0)
            .clamp(1.0, 10.0);
        let tags: Vec<String> = parts[3]
            .trim()
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let content = parts[4].trim().to_string();

        facts.push(ExtractedFact {
            content,
            importance,
            memory_type,
            tags,
            knowledge_triple: current_triple.take(),
        });
    }

    Ok(facts)
}
