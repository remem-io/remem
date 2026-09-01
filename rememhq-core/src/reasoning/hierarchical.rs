//! Hierarchical Memory Tree: Summary Facts -> Detailed Sub-Facts -> Source Citations.

pub use crate::memory::types::FactCitation;
use crate::memory::types::{MemoryRecord, MemoryType};
use crate::storage::MemoryStore;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A node in the hierarchical fact tree with recursive children and citations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactTreeNode {
    pub record: MemoryRecord,
    pub children: Vec<FactTreeNode>,
    pub depth: usize,
}

impl FactTreeNode {
    pub fn new(record: MemoryRecord, depth: usize) -> Self {
        Self {
            record,
            children: Vec::new(),
            depth,
        }
    }

    /// Render tree as a formatted outline string.
    pub fn render(&self) -> String {
        let indent = "  ".repeat(self.depth);
        let mut out = format!(
            "{}- [{}] {} (importance: {:.1})\n",
            indent, self.record.memory_type, self.record.content, self.record.importance
        );
        for cite in &self.record.citations {
            out.push_str(&format!(
                "{}  * [cite: {}] \"{}\"\n",
                indent, cite.source_type, cite.snippet
            ));
        }
        for child in &self.children {
            out.push_str(&child.render());
        }
        out
    }
}

/// Recursively load a hierarchical fact tree from any MemoryStore up to max_depth.
pub async fn get_fact_tree(
    store: &dyn MemoryStore,
    root_id: Uuid,
    max_depth: usize,
) -> anyhow::Result<Option<FactTreeNode>> {
    let root = match store.get(root_id).await? {
        Some(r) => r,
        None => return Ok(None),
    };

    let mut tree_root = FactTreeNode::new(root, 0);
    if max_depth > 0 {
        populate_children(store, &mut tree_root, max_depth).await?;
    }

    Ok(Some(tree_root))
}

async fn populate_children(
    store: &dyn MemoryStore,
    node: &mut FactTreeNode,
    remaining_depth: usize,
) -> anyhow::Result<()> {
    if remaining_depth == 0 {
        return Ok(());
    }

    let all_memories = store.list(&[], None, None, usize::MAX).await?;
    let children: Vec<MemoryRecord> = all_memories
        .into_iter()
        .filter(|m| m.parent_fact_id == Some(node.record.id))
        .collect();

    for child in children {
        let mut child_node = FactTreeNode::new(child, node.depth + 1);
        if remaining_depth > 1 {
            Box::pin(populate_children(
                store,
                &mut child_node,
                remaining_depth - 1,
            ))
            .await?;
        }
        node.children.push(child_node);
    }

    Ok(())
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
