pub mod validator;

use crate::providers::{ChatMessage, ChatResponse, Provider, Tool, ToolCall, ChatRole};
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
                // We just return the tool calls.
                return Ok(response);
            }

            let raw_json: Result<serde_json::Value, _> = serde_json::from_str(&response.message.content);
            match raw_json {
                Ok(val) => match validator.validate(&val) {
                    Ok(_) => return Ok(response),
                    Err(e) => {
                        if retries >= self.max_retries {
                            return Err(anyhow::anyhow!("Max retries reached. Last validation error: {}", e));
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
                        return Err(anyhow::anyhow!("Max retries reached. Failed to parse JSON: {}", e));
                    }
                    messages.push(response.message.clone());
                    messages.push(ChatMessage {
                        role: ChatRole::User,
                        content: format!("Failed to parse JSON: {}. Ensure you return valid JSON.", e),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
            retries += 1;
        }
    }
}
