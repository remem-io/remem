//! Bounded context fragments for structured LLM context assembly.
//!
//! Each fragment represents a distinct piece of context (project summary,
//! recent memories, knowledge graph excerpts, session history) with explicit
//! token budgets and priority ordering. When assembling context for an LLM
//! call, fragments are sorted by priority and packed greedily within the
//! overall token budget.

use serde::{Deserialize, Serialize};

/// A bounded, prioritised unit of context for LLM consumption.
///
/// Every piece of injected context must implement this trait so that the
/// context builder can enforce per-fragment token caps and prioritise
/// inclusion when the overall budget is tight.
pub trait ContextFragment: Send + Sync {
    /// Human-readable label for this fragment (e.g. "Recent Sessions").
    fn label(&self) -> &str;

    /// Maximum tokens this fragment is allowed to consume.
    fn max_tokens(&self) -> usize;

    /// Priority for inclusion when budget is tight.
    /// Higher values are kept first (0 = lowest, 255 = highest).
    fn priority(&self) -> u8;

    /// Render the fragment content, truncated to fit within `token_budget`.
    ///
    /// The implementation should produce at most `token_budget` estimated
    /// tokens of output. Use [`estimate_tokens`] for the heuristic.
    fn render(&self, token_budget: usize) -> String;
}

/// Very rough heuristic: ~4 characters per token for English text.
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// A simple text-based context fragment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextFragment {
    label: String,
    content: String,
    max_tokens: usize,
    priority: u8,
}

impl TextFragment {
    pub fn new(
        label: impl Into<String>,
        content: impl Into<String>,
        max_tokens: usize,
        priority: u8,
    ) -> Self {
        Self {
            label: label.into(),
            content: content.into(),
            max_tokens,
            priority,
        }
    }
}

impl ContextFragment for TextFragment {
    fn label(&self) -> &str {
        &self.label
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn render(&self, token_budget: usize) -> String {
        let budget = token_budget.min(self.max_tokens);
        let max_chars = budget * 4; // inverse of estimate_tokens
        if self.content.len() <= max_chars {
            self.content.clone()
        } else {
            // Truncate at char boundary
            let truncated: String = self
                .content
                .chars()
                .take(max_chars.saturating_sub(4))
                .collect();
            format!("{}...", truncated)
        }
    }
}

/// Assembles multiple [`ContextFragment`]s into a single context string
/// within a total token budget.
///
/// Fragments are sorted by priority (highest first) and packed greedily.
/// Each fragment receives the minimum of its own `max_tokens` and the
/// remaining budget.
pub fn assemble_fragments(
    fragments: &mut [Box<dyn ContextFragment>],
    total_budget: usize,
) -> String {
    // Sort by priority descending
    fragments.sort_by(|a, b| b.priority().cmp(&a.priority()));

    let mut parts = Vec::new();
    let mut remaining = total_budget;

    for fragment in fragments.iter() {
        if remaining == 0 {
            break;
        }

        let budget = remaining.min(fragment.max_tokens());
        let rendered = fragment.render(budget);
        let used = estimate_tokens(&rendered);

        if used > 0 {
            parts.push(format!("## {}\n{}", fragment.label(), rendered));
            remaining = remaining.saturating_sub(used);
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_text_fragment_render_within_budget() {
        let frag = TextFragment::new("Test", "Hello world", 100, 50);
        let rendered = frag.render(100);
        assert_eq!(rendered, "Hello world");
    }

    #[test]
    fn test_text_fragment_render_truncated() {
        let content = "a".repeat(1000);
        let frag = TextFragment::new("Test", content, 10, 50);
        let rendered = frag.render(10);
        // 10 tokens * 4 chars = 40 chars max, minus 4 for "..." = 36 chars + "..."
        assert!(rendered.len() <= 40);
        assert!(rendered.ends_with("..."));
    }

    #[test]
    fn test_assemble_fragments_priority_ordering() {
        let mut fragments: Vec<Box<dyn ContextFragment>> = vec![
            Box::new(TextFragment::new("Low", "low priority content", 100, 10)),
            Box::new(TextFragment::new("High", "high priority content", 100, 90)),
            Box::new(TextFragment::new("Mid", "mid priority content", 100, 50)),
        ];

        let result = assemble_fragments(&mut fragments, 10000);

        // High priority should come first
        let high_pos = result.find("High").unwrap();
        let mid_pos = result.find("Mid").unwrap();
        let low_pos = result.find("Low").unwrap();
        assert!(high_pos < mid_pos);
        assert!(mid_pos < low_pos);
    }

    #[test]
    fn test_assemble_fragments_budget_exhaustion() {
        let mut fragments: Vec<Box<dyn ContextFragment>> = vec![
            Box::new(TextFragment::new("First", "a".repeat(400), 200, 90)), // 100 tokens
            Box::new(TextFragment::new("Second", "b".repeat(400), 200, 50)), // 100 tokens
        ];

        // Budget only allows ~120 tokens total — first fragment takes ~100+header,
        // leaving very little for the second
        let result = assemble_fragments(&mut fragments, 120);
        assert!(result.contains("First"));
    }
}
