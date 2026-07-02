//! Context compaction — distil conversation traces into high-fidelity summaries.
//!
//! Helps AI agents manage their context windows by summarizing older messages
//! while preserving critical architectural decisions, bugs, and states, and
//! stripping out redundant tool outputs.

use crate::providers::{Provider, ProviderOptions};

/// A report of the compaction process.
#[derive(Debug)]
pub struct CompactionReport {
    pub compressed_context: String,
    pub original_length: usize,
    pub compressed_length: usize,
}

/// Compact a conversation trace to save context window tokens.
///
/// This operation processes a raw conversation trace that is nearing the agent's context
/// window limit, and condenses it into a high-fidelity summary. It focuses on preserving
/// critical architectural decisions, unresolved bugs, implementation details, and project
/// states, while stripping out redundant tool outputs and ephemeral conversation filler.
///
/// An optional list of `focus_areas` can be provided to guarantee that certain topics
/// (e.g. specific open issues or tasks) are prioritized in the summary output.
pub async fn compact_context(
    provider: &dyn Provider,
    model: &str,
    conversation_text: &str,
    focus_areas: Option<&[String]>,
    options: Option<&ProviderOptions>,
) -> anyhow::Result<CompactionReport> {
    let focus_prompt = if let Some(areas) = focus_areas {
        if !areas.is_empty() {
            format!(
                "\n\nEnsure you pay special attention to preserving details about the following focus areas:\n- {}",
                areas.join("\n- ")
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let prompt = format!(
        r#"You are a context compaction engine for an AI agent. 
Your task is to take a raw conversation trace that is nearing the agent's context window limit and summarize its contents into a dense, high-fidelity format.

CRITICAL INSTRUCTIONS:
1. Preserve architectural decisions, unresolved bugs, implementation details, and critical project state.
2. Discard redundant tool outputs, repetitive errors that have been resolved, and superfluous conversational filler.
3. Keep the output as dense as possible while ensuring the agent can continue its task without losing important context.
4. Output ONLY the compressed summary. Do not include introductory or concluding text.{focus_prompt}

Conversation Trace:
{conversation_text}

Output the compressed context now:"#
    );

    let (compressed_context, _usage) = provider.complete(&prompt, model, options).await?;

    Ok(CompactionReport {
        compressed_context: compressed_context.trim().to_string(),
        original_length: conversation_text.len(),
        compressed_length: compressed_context.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

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
                "Compressed summary".to_string(),
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
    async fn test_compact_context() {
        let provider = MockProviderObj;
        let trace = "This is a very long conversation trace that needs to be compacted.";
        let report = compact_context(&provider, "mock", trace, None, None)
            .await
            .unwrap();

        assert_eq!(report.compressed_context, "Compressed summary");
        assert_eq!(report.original_length, trace.len());
        assert_eq!(report.compressed_length, 18); // "Compressed summary".len()
    }
}
