use super::AgentLoop;
use crate::harness::AgentHarness;
use crate::providers::{ChatMessage, ChatRole};
use async_trait::async_trait;

pub struct GenerateEvaluateRefineLoop {
    pub harness: AgentHarness,
    pub task: String,
    pub max_iterations: usize,
    pub maker_model: String,
    pub checker_model: String,
}

impl GenerateEvaluateRefineLoop {
    pub fn new(
        harness: AgentHarness,
        task: String,
        maker_model: String,
        checker_model: String,
    ) -> Self {
        Self {
            harness,
            task,
            max_iterations: 5,
            maker_model,
            checker_model,
        }
    }
}

#[async_trait]
impl AgentLoop for GenerateEvaluateRefineLoop {
    async fn run(&mut self) -> anyhow::Result<String> {
        let mut messages = vec![
            ChatMessage {
                role: ChatRole::System,
                content: "You are the maker. Please solve the task.".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: self.task.clone(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let mut current_solution = String::new();

        for _ in 0..self.max_iterations {
            let response = self
                .harness
                .provider
                .chat(&messages, &self.harness.tools, &self.maker_model, None)
                .await?;
            current_solution = response.message.content.clone();
            messages.push(response.message);

            // Evaluator step
            let eval_messages = vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: "You are an evaluator. Evaluate the provided solution against the original task. If it is fully complete, correct, and needs no changes, respond with exactly 'PASS'. Otherwise, provide constructive criticism.".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: format!("Task: {}\nSolution: {}", self.task, current_solution),
                    tool_calls: None,
                    tool_call_id: None,
                }
            ];

            let eval_response = self
                .harness
                .provider
                .chat(&eval_messages, &[], &self.checker_model, None)
                .await?;
            if eval_response.message.content.trim() == "PASS" {
                return Ok(current_solution);
            } else {
                messages.push(ChatMessage {
                    role: ChatRole::User,
                    content: format!(
                        "Your solution failed evaluation: {}. Please refine it.",
                        eval_response.message.content
                    ),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        Ok(current_solution)
    }
}
