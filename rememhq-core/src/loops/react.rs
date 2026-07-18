use super::AgentLoop;
use crate::harness::AgentHarness;
use crate::providers::{ChatMessage, ChatRole};
use crate::reasoning::ReasoningEngine;
use async_trait::async_trait;
use std::sync::Arc;

pub struct ReActLoop {
    pub harness: AgentHarness,
    pub engine: Arc<ReasoningEngine>,
    pub max_iterations: usize,
    pub task: String,
    pub messages: Vec<ChatMessage>,
}

impl ReActLoop {
    pub fn new(harness: AgentHarness, engine: Arc<ReasoningEngine>, task: String) -> Self {
        Self {
            harness,
            engine,
            max_iterations: 10,
            task,
            messages: Vec::new(),
        }
    }
}

#[async_trait]
impl AgentLoop for ReActLoop {
    async fn run(&mut self) -> anyhow::Result<String> {
        self.messages.push(ChatMessage {
            role: ChatRole::System,
            content: "You are an autonomous agent using the ReAct (Reason + Act) pattern. You must loop until the task is complete. If you are done, return your final answer as text without calling any tools.".to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        self.messages.push(ChatMessage {
            role: ChatRole::User,
            content: self.task.clone(),
            tool_calls: None,
            tool_call_id: None,
        });

        for _ in 0..self.max_iterations {
            let response = self
                .harness
                .provider
                .chat(
                    &self.messages,
                    &self.harness.tools,
                    &self.engine.config.reasoning.reasoning_model,
                    None,
                )
                .await?;
            self.messages.push(response.message.clone());

            if let Some(tool_calls) = response.message.tool_calls {
                if let Some(executor) = &self.harness.executor {
                    for tool_call in tool_calls {
                        let result = match executor.execute(&tool_call).await {
                            Ok(res) => res,
                            Err(e) => format!("Error executing tool: {}", e),
                        };
                        self.messages.push(ChatMessage {
                            role: ChatRole::Tool,
                            content: result,
                            tool_calls: None,
                            tool_call_id: Some(tool_call.id),
                        });
                    }
                } else {
                    self.messages.push(ChatMessage {
                        role: ChatRole::System,
                        content: "No tool executor available.".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            } else {
                // Assume done if no tools are called
                return Ok(response.message.content);
            }

            // Context management: compact older turns once history gets large,
            // so long-running loops don't grow the context window unboundedly.
            if self.messages.len() > 20 {
                compact_history(
                    self.harness.provider.as_ref(),
                    &self.engine.config.reasoning.reasoning_model,
                    &mut self.messages,
                )
                .await;
            }
        }

        Err(anyhow::anyhow!(
            "Max iterations reached without completion in ReAct Loop"
        ))
    }
}

/// Compact everything in `messages` after the leading system prompt and task
/// message (indices 0 and 1) into a single summary message, using the
/// existing `compact_context` reasoning primitive.
///
/// Replacing the *entire* tail in one shot (rather than partially) guarantees
/// we never strand an assistant `tool_calls` message without its matching
/// `Tool`-role responses, which some providers require to stay paired.
///
/// If compaction fails (e.g. a transient provider error), `messages` is left
/// untouched and the loop simply tries again once more history has built up.
async fn compact_history(provider: &dyn crate::providers::Provider, model: &str, messages: &mut Vec<ChatMessage>) {
    if messages.len() <= 2 {
        return;
    }

    tracing::info!(
        message_count = messages.len(),
        "Context size large, compacting older turns"
    );

    let conversation_text: String = messages[2..]
        .iter()
        .map(|m| format!("[{:?}] {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    match crate::reasoning::compaction::compact_context(provider, model, &conversation_text, None, None).await {
        Ok(report) => {
            messages.truncate(2);
            messages.push(ChatMessage {
                role: ChatRole::System,
                content: format!(
                    "[Earlier conversation history was compacted to save context. Summary of what happened so far:]\n{}",
                    report.compressed_context
                ),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        Err(e) => {
            // Don't fail the loop over a failed compaction attempt — just let
            // history keep growing and try again next iteration.
            tracing::warn!("Context compaction failed, continuing without it: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderOptions, TokenUsage};
    use async_trait::async_trait;

    struct MockProviderObj {
        response: String,
    }

    #[async_trait]
    impl crate::providers::Provider for MockProviderObj {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<TokenUsage>)> {
            Ok((self.response.clone(), None))
        }
        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[crate::providers::Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<crate::providers::ChatResponse> {
            unimplemented!("compact_history only calls complete()")
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    fn msg(role: ChatRole, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[tokio::test]
    async fn test_compact_history_replaces_tail_and_keeps_head() {
        let provider = MockProviderObj {
            response: "Summary of everything that happened.".to_string(),
        };
        let mut messages = vec![
            msg(ChatRole::System, "system prompt"),
            msg(ChatRole::User, "the task"),
            msg(ChatRole::Assistant, "turn 1"),
            msg(ChatRole::User, "turn 2"),
            msg(ChatRole::Assistant, "turn 3"),
        ];

        compact_history(&provider, "mock", &mut messages).await;

        assert_eq!(messages.len(), 3, "head (2) + one summary message");
        assert_eq!(messages[0].content, "system prompt");
        assert_eq!(messages[1].content, "the task");
        assert_eq!(messages[2].role, ChatRole::System);
        assert!(messages[2].content.contains("Summary of everything that happened."));
    }

    #[tokio::test]
    async fn test_compact_history_no_op_when_only_head_present() {
        let provider = MockProviderObj {
            response: "should not be used".to_string(),
        };
        let mut messages = vec![
            msg(ChatRole::System, "system prompt"),
            msg(ChatRole::User, "the task"),
        ];

        compact_history(&provider, "mock", &mut messages).await;

        // Nothing to compact yet; must be left exactly as-is.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "system prompt");
        assert_eq!(messages[1].content, "the task");
    }

    struct FailingProviderObj;
    #[async_trait]
    impl crate::providers::Provider for FailingProviderObj {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<TokenUsage>)> {
            Err(anyhow::anyhow!("provider unavailable"))
        }
        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[crate::providers::Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<crate::providers::ChatResponse> {
            unimplemented!()
        }
        fn name(&self) -> &str {
            "failing_mock"
        }
    }

    #[tokio::test]
    async fn test_compact_history_leaves_messages_untouched_on_provider_error() {
        let provider = FailingProviderObj;
        let mut messages = vec![
            msg(ChatRole::System, "system prompt"),
            msg(ChatRole::User, "the task"),
            msg(ChatRole::Assistant, "turn 1"),
        ];
        let original = messages.clone();

        compact_history(&provider, "mock", &mut messages).await;

        assert_eq!(
            messages.len(),
            original.len(),
            "a failed compaction attempt must not lose or alter history"
        );
        for (a, b) in messages.iter().zip(original.iter()) {
            assert_eq!(a.content, b.content);
        }
    }
}
