pub mod validator;

use crate::providers::{ChatMessage, ChatResponse, ChatRole, Provider, Tool, ToolCall};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool call safely within a harness.
    async fn execute(&self, tool_call: &ToolCall) -> anyhow::Result<String>;
}

/// Harness represents the scaffolding that constrains, validates, and operationalizes
/// model behavior. It wraps the core Provider to provide structured outputs and tool execution bounds.
pub struct AgentHarness {
    pub provider: Arc<dyn Provider>,
    pub max_retries: usize,
    pub tools: Vec<Tool>,
    pub executor: Option<Arc<dyn ToolExecutor>>,
}

impl AgentHarness {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            max_retries: 3,
            tools: Vec::new(),
            executor: None,
        }
    }

    pub fn with_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn with_tools(mut self, tools: Vec<Tool>, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tools = tools;
        self.executor = Some(executor);
        self
    }

    /// Enforces output validation. If the model fails to produce valid output,
    /// it retries up to `max_retries` times, injecting the error message into context.
    pub async fn chat_with_validation<V: validator::Validator>(
        &self,
        messages: &mut Vec<ChatMessage>,
        model: &str,
        validator: &V,
        options: Option<&crate::providers::ProviderOptions>,
    ) -> anyhow::Result<ChatResponse> {
        let mut retries = 0;
        loop {
            let response = self
                .provider
                .chat(messages, &self.tools, model, options)
                .await?;

            if response.message.tool_calls.is_some() {
                // If the model tried to call tools, we don't validate the raw text output here.
                // We just return the tool calls, after recording the turn like every other
                // exit path does (see note on the success branch below).
                messages.push(response.message.clone());
                return Ok(response);
            }

            let raw_json: Result<serde_json::Value, _> =
                serde_json::from_str(&response.message.content);
            match raw_json {
                Ok(val) => match validator.validate(&val) {
                    Ok(_) => {
                        // `messages` is `&mut` specifically so the caller's conversation
                        // history stays in sync with what was actually sent/received.
                        // The failure branches below already push both the rejected
                        // assistant turn and the correction prompt; the success path has
                        // to push the accepted assistant turn too, or callers that keep
                        // reusing `messages` for a follow-up turn silently lose it.
                        messages.push(response.message.clone());
                        return Ok(response);
                    }
                    Err(e) => {
                        if retries >= self.max_retries {
                            return Err(anyhow::anyhow!(
                                "Max retries reached. Last validation error: {}",
                                e
                            ));
                        }
                        messages.push(response.message.clone());
                        messages.push(ChatMessage {
                            role: ChatRole::User,
                            content: format!("Your output failed validation: {}. Please fix the errors and provide a valid JSON output.", e),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                },
                Err(e) => {
                    if retries >= self.max_retries {
                        return Err(anyhow::anyhow!(
                            "Max retries reached. Failed to parse JSON: {}",
                            e
                        ));
                    }
                    messages.push(response.message.clone());
                    messages.push(ChatMessage {
                        role: ChatRole::User,
                        content: format!(
                            "Failed to parse JSON: {}. Ensure you return valid JSON.",
                            e
                        ),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
            retries += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::validator::SchemaValidator;
    use crate::providers::{ProviderOptions, TokenUsage};
    use std::sync::Mutex;

    /// A provider that returns queued `ChatResponse`s in order, one per `.chat()` call.
    /// Panics if called more times than responses were queued, so tests fail loudly
    /// on unexpected extra retries instead of hanging or reusing stale data.
    struct QueueProvider {
        responses: Mutex<std::collections::VecDeque<ChatResponse>>,
    }

    impl QueueProvider {
        fn new(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
            }
        }
    }

    fn assistant_msg(content: &str) -> ChatMessage {
        ChatMessage {
            role: ChatRole::Assistant,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[async_trait]
    impl Provider for QueueProvider {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<TokenUsage>)> {
            unimplemented!("QueueProvider only supports chat() in these tests")
        }

        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<ChatResponse> {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("QueueProvider ran out of queued responses"))
        }

        fn name(&self) -> &str {
            "queue"
        }
    }

    fn name_validator() -> SchemaValidator {
        SchemaValidator::new(vec![("name".to_string(), "string".to_string())])
    }

    #[tokio::test]
    async fn test_success_on_first_try_appends_message_to_history() {
        let provider = QueueProvider::new(vec![ChatResponse {
            message: assistant_msg(r#"{"name": "Alice"}"#),
            usage: None,
        }]);
        let harness = AgentHarness::new(Arc::new(provider));
        let mut messages = vec![ChatMessage {
            role: ChatRole::User,
            content: "who is this?".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let result = harness
            .chat_with_validation(&mut messages, "mock", &name_validator(), None)
            .await
            .unwrap();

        assert_eq!(result.message.content, r#"{"name": "Alice"}"#);
        // Regression: the accepted assistant turn must land in the caller's
        // conversation history, not just be returned and then silently dropped.
        assert_eq!(messages.len(), 2, "assistant response was not appended");
        assert_eq!(messages[1].content, r#"{"name": "Alice"}"#);
        assert_eq!(messages[1].role, ChatRole::Assistant);
    }

    #[tokio::test]
    async fn test_invalid_json_then_valid_retries_and_succeeds() {
        let provider = QueueProvider::new(vec![
            ChatResponse {
                message: assistant_msg("not json at all"),
                usage: None,
            },
            ChatResponse {
                message: assistant_msg(r#"{"name": "Bob"}"#),
                usage: None,
            },
        ]);
        let harness = AgentHarness::new(Arc::new(provider));
        let mut messages = vec![];

        let result = harness
            .chat_with_validation(&mut messages, "mock", &name_validator(), None)
            .await
            .unwrap();

        assert_eq!(result.message.content, r#"{"name": "Bob"}"#);
        // Failed attempt + correction prompt + final accepted assistant turn.
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "not json at all");
        assert_eq!(messages[1].role, ChatRole::User); // correction prompt
        assert_eq!(messages[2].content, r#"{"name": "Bob"}"#);
    }

    #[tokio::test]
    async fn test_exhausts_retries_returns_err() {
        // max_retries = 1 -> at most 2 total attempts, both invalid.
        let provider = QueueProvider::new(vec![
            ChatResponse {
                message: assistant_msg("still not json"),
                usage: None,
            },
            ChatResponse {
                message: assistant_msg("still not json"),
                usage: None,
            },
        ]);
        let harness = AgentHarness::new(Arc::new(provider)).with_retries(1);
        let mut messages = vec![];

        let result = harness
            .chat_with_validation(&mut messages, "mock", &name_validator(), None)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tool_calls_bypass_validation_and_are_recorded() {
        let provider = QueueProvider::new(vec![ChatResponse {
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: "".to_string(),
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "search".to_string(),
                    arguments: serde_json::json!({"query": "rust"}),
                }]),
                tool_call_id: None,
            },
            usage: None,
        }]);
        let harness = AgentHarness::new(Arc::new(provider));
        let mut messages = vec![];

        let result = harness
            .chat_with_validation(&mut messages, "mock", &name_validator(), None)
            .await
            .unwrap();

        assert!(result.message.tool_calls.is_some());
        // Tool-call turns skip JSON validation entirely but must still be recorded.
        assert_eq!(messages.len(), 1);
        assert!(messages[0].tool_calls.is_some());
    }
}
