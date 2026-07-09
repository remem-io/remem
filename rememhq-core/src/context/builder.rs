// Unused imports removed
use crate::reasoning::ReasoningEngine;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDocument {
    pub header: String,
    pub timeline: String,
    pub facts: String,
    pub knowledge_graph: String,
    pub footer: String,
}

impl ContextDocument {
    pub fn to_string_formatted(&self) -> String {
        format!(
            "{}\n\n{}\n\n{}\n\n{}\n\n{}",
            self.header, self.timeline, self.facts, self.knowledge_graph, self.footer
        )
    }
}

pub struct ContextBuilder<'a> {
    engine: &'a ReasoningEngine,
    token_budget: usize,
    project: String,
}

impl<'a> ContextBuilder<'a> {
    pub fn new(engine: &'a ReasoningEngine, project: &str, token_budget: usize) -> Self {
        Self {
            engine,
            token_budget,
            project: project.to_string(),
        }
    }

    /// Very rough heuristic: 4 chars per token for English text
    fn estimate_tokens(text: &str) -> usize {
        text.len() / 4
    }

    pub async fn build(&self) -> anyhow::Result<ContextDocument> {
        // 1. Gather recent sessions
        let recent_sessions = self
            .engine
            .store
            .get_recent_session_summaries(&self.project, 10)
            .await
            .unwrap_or_default();

        // 2. Gather top facts
        use crate::storage::MemoryStore;
        let mut all_memories = self
            .engine
            .store
            .list(&[], None, None, 100)
            .await
            .unwrap_or_default();

        // Sort by importance (descending) and recency
        let now = Utc::now();
        all_memories.sort_by(|a, b| {
            let age_a = (now - a.updated_at).num_days() as f32;
            let age_b = (now - b.updated_at).num_days() as f32;
            let score_a = a.importance / (1.0 + age_a * 0.1);
            let score_b = b.importance / (1.0 + age_b * 0.1);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 3. Gather knowledge graph context (just top 20 triples for now)
        let mut triples_query = self
            .engine
            .store
            .query_knowledge(None, None, None)
            .await
            .unwrap_or_default();
        triples_query.truncate(20);

        // Build the document parts, checking budget
        let header = format!("# Context for Project: {}\n", self.project);

        let mut timeline = String::from("## Recent Sessions\n");
        for session in recent_sessions {
            let entry = format!(
                "- [{}]: {}\n",
                session.timestamp.format("%Y-%m-%d %H:%M"),
                session.summary
            );
            if Self::estimate_tokens(&timeline) + Self::estimate_tokens(&entry)
                > self.token_budget / 4
            {
                break;
            }
            timeline.push_str(&entry);
        }

        let mut facts = String::from("## Key Facts & Observations\n");
        for mem in all_memories {
            let entry = format!("- [{}] {}\n", mem.memory_type, mem.content);
            if Self::estimate_tokens(&facts) + Self::estimate_tokens(&entry) > self.token_budget / 2
            {
                break;
            }
            facts.push_str(&entry);
        }

        let mut kg = String::from("## Knowledge Graph Summary\n");
        for t in triples_query {
            let entry = format!("- {} -> {} -> {}\n", t.subject, t.predicate, t.object);
            if Self::estimate_tokens(&kg) + Self::estimate_tokens(&entry) > self.token_budget / 4 {
                break;
            }
            kg.push_str(&entry);
        }

        let footer = String::from("--- End of Context ---");

        Ok(ContextDocument {
            header,
            timeline,
            facts,
            knowledge_graph: kg,
            footer,
        })
    }
}
