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

            // Context management: Trigger compaction if context gets too big
            if self.messages.len() > 20 {
                tracing::info!("Context size large. Delegating compaction to ReasoningEngine.");
                // Compact logic could be hooked here. For now, we let it grow but emit a warning.
            }
        }

        Err(anyhow::anyhow!(
            "Max iterations reached without completion in ReAct Loop"
        ))
    }
}
