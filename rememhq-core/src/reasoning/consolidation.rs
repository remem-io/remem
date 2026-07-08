//! Consolidation — extract durable facts from raw session interactions.
//!
//! When a session ends, the consolidation engine uses an LLM to:
//! 1. Extract durable facts from the raw interaction log
//! 2. Score each fact's importance
//! 3. Detect contradictions with existing memories
//! 4. Update the knowledge graph

use crate::memory::types::{ConsolidationReport, KnowledgeGraphUpdate, MemoryRecord, MemoryType};
use crate::providers::{EmbeddingProvider, Provider, ProviderOptions};
use crate::storage::sqlite::SqliteStore;
use crate::storage::vector::VectorIndex;
use crate::storage::MemoryStore;
use serde::{Deserialize, Serialize};

/// Run a consolidation pass over a session's memories.
///
/// At the end of an interaction session, the consolidation engine extracts long-term
/// value from the raw memory trace. It performs a sequence of AI-driven reasoning steps:
///
/// 1. **Fact Extraction**: The LLM parses the session transcript to identify durable facts
///    and procedures, scoring their initial importance.
/// 2. **Entity Resolution (Knowledge Graph)**: It extracts semantic triples (`subject -> predicate -> object`)
///    and resolves entities (e.g. standardizing "Anthropic Provider" to "Anthropic") by
///    checking the existing knowledge graph.
/// 3. **Contradiction Detection**: Facts are compared against the existing memory store using
///    a vector index to discover contradictions, resolving them in favor of newer information.
/// 4. **Storage**: The consolidated facts are indexed and written to the persistent store.
pub async fn consolidate_session(
    provider: &dyn Provider,
    embeddings: &dyn EmbeddingProvider,
    store: &SqliteStore,
    index: &dyn VectorIndex,
    session_id: &str,
    model: &str,
    options: Option<&ProviderOptions>,
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
    // Use the LLM to identify durable knowledge and discard conversational noise.
    let mut facts = extract_facts(provider, &session_content, model, options).await?;

    // Step 1b: Resolve entities in Knowledge Graph triples
    // Use the LLM to perform semantic entity resolution, ensuring that entities like
    // "Remem" and "remem-io" are mapped to the canonical entity name in the store.
    let resolver =
        super::resolution::LlmEntityResolver::new(provider, model.to_string(), store, options);
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
        provider, embeddings, index, store, &facts, model, options,
    )
    .await?;

    let mut inserts = Vec::new();
    let mut updates = Vec::new();
    let mut archives = Vec::new();
    let mut triples = Vec::new();
    let mut index_adds = Vec::new();

    // Auto-resolve contradictions by preparing archives
    for c in &contradictions {
        tracing::info!(
            old_memory_id = %c.existing_memory_id,
            explanation = %c.explanation,
            "Auto-resolving contradiction by preparing archive of superseded memory"
        );
        archives.push(c.existing_memory_id);
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
        let embedding = embeddings.embed(&record.content, options).await?;
        record.embedding = Some(embedding.clone());

        // Check if this fact updates an existing memory
        let existing_results = index.search(&embedding, 3).await?;
        let mut is_update = false;

        for er in &existing_results {
            if er.similarity > 0.92 && !archives.contains(&er.id) {
                // Very similar — this is an update, not a new fact
                if let Ok(Some(existing)) = store.get(er.id).await {
                    let mut updated = existing;
                    updated.content = record.content.clone();
                    updated.importance = record.importance.max(updated.importance);
                    updated.updated_at = chrono::Utc::now();

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

        // Extract knowledge graph triples
        if let Some(triple) = &fact.knowledge_triple {
            kg_updates.push(triple.clone());
            triples.push((triple.clone(), record.id));
        }
    }

    // Execute ALL SQLite writes atomically in a single transaction
    store
        .save_consolidation(&inserts, &updates, &archives, &triples)
        .await?;

    // Add to vector index after successful DB commit
    for (id, embedding) in index_adds {
        let _ = index.add(id, &embedding).await;
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
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractedFact {
    pub content: String,
    pub importance: f32,
    pub memory_type: MemoryType,
    pub tags: Vec<String>,
    pub knowledge_triple: Option<KnowledgeGraphUpdate>,
}

/// Use the LLM to extract durable facts from raw session content.
// Note: exposed for session compression
pub async fn extract_facts(
    provider: &dyn Provider,
    session_content: &str,
    model: &str,
    options: Option<&ProviderOptions>,
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

    let (response, _usage) = provider.complete(&prompt, model, options).await?;

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

/// Use the LLM to generate a structured summary of a session.
pub async fn generate_session_summary(
    provider: &dyn Provider,
    session_id: &str,
    project: &str,
    session_content: &str,
    model: &str,
    options: Option<&ProviderOptions>,
) -> anyhow::Result<crate::memory::types::SessionSummaryRecord> {
    let prompt = format!(
        r#"You are a memory consolidation engine. Your task is to generate a concise summary of the following session log.

Please provide your output in the following JSON format ONLY, with no additional text:
{{
  "summary": "A one-paragraph summary of what was accomplished in this session.",
  "files_touched": ["file1.rs", "file2.ts"],
  "key_decisions": ["Decided to use SQLite instead of Postgres", "Added a new observation type"]
}}

Session log:
{session_content}
"#
    );

    let (response, _usage) = provider.complete(&prompt, model, options).await?;

    // Extract JSON block if surrounded by backticks
    let json_text = if let Some(start) = response.find("```json") {
        if let Some(end) = response[start + 7..].find("```") {
            &response[start + 7..start + 7 + end]
        } else {
            &response[start + 7..]
        }
    } else if let Some(start) = response.find("```") {
        if let Some(end) = response[start + 3..].find("```") {
            &response[start + 3..start + 3 + end]
        } else {
            &response[start + 3..]
        }
    } else {
        &response
    };

    #[derive(serde::Deserialize)]
    struct SummaryOutput {
        summary: String,
        #[serde(default)]
        files_touched: Vec<String>,
        #[serde(default)]
        key_decisions: Vec<String>,
    }

    let parsed: SummaryOutput = serde_json::from_str(json_text.trim())
        .unwrap_or_else(|_| SummaryOutput {
            summary: "Failed to parse session summary.".to_string(),
            files_touched: vec![],
            key_decisions: vec![],
        });

    Ok(crate::memory::types::SessionSummaryRecord {
        session_id: session_id.to_string(),
        project: project.to_string(),
        summary: parsed.summary,
        files_touched: parsed.files_touched,
        key_decisions: parsed.key_decisions,
        timestamp: chrono::Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

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
    async fn test_extract_facts_parsing() {
        let provider = MockProviderObj {
            response: "FACT | fact | 8 | rust, test | This is a test fact\nTRIPLE | subject | pred | obj\nFACT | procedure | 9 | dev | This is a procedure".to_string(),
        };

        let facts = extract_facts(&provider, "session logs", "mock", None)
            .await
            .unwrap();

        assert_eq!(facts.len(), 2);

        assert_eq!(facts[0].content, "This is a test fact");
        assert_eq!(facts[0].importance, 8.0);
        assert_eq!(facts[0].memory_type, MemoryType::Fact);
        assert_eq!(facts[0].tags, vec!["rust", "test"]);
        assert!(facts[0].knowledge_triple.is_none());

        assert_eq!(facts[1].content, "This is a procedure");
        assert_eq!(facts[1].importance, 9.0);
        assert_eq!(facts[1].memory_type, MemoryType::Procedure);
        assert_eq!(facts[1].tags, vec!["dev"]);

        let triple = facts[1].knowledge_triple.as_ref().unwrap();
        assert_eq!(triple.subject, "subject");
        assert_eq!(triple.predicate, "pred");
        assert_eq!(triple.object, "obj");
    }

    #[tokio::test]
    async fn test_extract_facts_empty() {
        let provider = MockProviderObj {
            response: "Some conversational text without FACT lines".to_string(),
        };

        let facts = extract_facts(&provider, "session logs", "mock", None)
            .await
            .unwrap();
        assert!(facts.is_empty());
    }
}
