//! Hierarchical Memory Tree: Summary Facts -> Detailed Sub-Facts -> Source Citations.

use crate::memory::types::MemoryType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Source citation linking a fact to an exact interaction or document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactCitation {
    pub source_id: String,
    pub source_type: String, // "session_observation", "file", "url", "user_prompt"
    pub snippet: String,
    pub timestamp: DateTime<Utc>,
}

/// A fine-grained sub-fact providing specific context to a parent summary fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubFact {
    pub id: Uuid,
    pub content: String,
    pub detail_level: u8, // 1 = high-level nuance, 2 = deep technical detail
    pub citations: Vec<FactCitation>,
}

/// A root-level summary fact that anchors fine-grained sub-facts and citations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalFact {
    pub id: Uuid,
    pub summary: String,
    pub memory_type: MemoryType,
    pub importance: f32,
    pub tags: Vec<String>,
    pub sub_facts: Vec<SubFact>,
    pub created_at: DateTime<Utc>,
}

impl HierarchicalFact {
    pub fn new(summary: impl Into<String>, memory_type: MemoryType, importance: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            summary: summary.into(),
            memory_type,
            importance,
            tags: Vec::new(),
            sub_facts: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Attach a detailed sub-fact to this hierarchical node.
    pub fn add_sub_fact(
        &mut self,
        content: impl Into<String>,
        detail_level: u8,
        citations: Vec<FactCitation>,
    ) {
        self.sub_facts.push(SubFact {
            id: Uuid::new_v4(),
            content: content.into(),
            detail_level,
            citations,
        });
    }

    /// Render a multi-level Markdown outline of this memory node.
    pub fn render_outline(&self) -> String {
        let mut out = format!(
            "- **{}** (importance: {:.1})\n",
            self.summary, self.importance
        );
        for sub in &self.sub_facts {
            out.push_str(&format!("  * {}\n", sub.content));
            for cite in &sub.citations {
                out.push_str(&format!(
                    "    - [cite: {}] \"{}\"\n",
                    cite.source_type, cite.snippet
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hierarchical_fact_outline() {
        let mut fact = HierarchicalFact::new(
            "User is building a Rust memory layer",
            MemoryType::Fact,
            9.0,
        );
        fact.add_sub_fact(
            "Uses HNSW for vector search and SQLite for relational triples",
            1,
            vec![FactCitation {
                source_id: "obs-1".into(),
                source_type: "session_observation".into(),
                snippet: "libremem C++ FFI bridge".into(),
                timestamp: Utc::now(),
            }],
        );

        let outline = fact.render_outline();
        assert!(outline.contains("User is building a Rust memory layer"));
        assert!(outline.contains("Uses HNSW"));
        assert!(outline.contains("libremem C++ FFI bridge"));
    }
}
