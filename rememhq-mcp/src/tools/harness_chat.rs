use rememhq_core::harness::{AgentHarness, Permissions};
use rememhq_core::providers::{ChatMessage, ChatRole};
use rememhq_core::reasoning::ReasoningEngine;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn schema() -> Value {
    json!({
        "name": "mem_harness_chat",
        "description": "Send a chat message through a highly-constrained Agent Harness with explicit retry logic and permissions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The user prompt to send to the harness."
                },
                "model": {
                    "type": "string",
                    "description": "The reasoning model to use (e.g., claude-sonnet-4-5)."
                },
                "allow_file_read": {
                    "type": "boolean",
                    "description": "Whether the agent is permitted to read files.",
                    "default": true
                },
                "allow_file_write": {
                    "type": "boolean",
                    "description": "Whether the agent is permitted to write files.",
                    "default": false
                },
                "max_retries": {
                    "type": "integer",
                    "description": "Maximum number of retries if the provider fails.",
                    "default": 3
                }
            },
            "required": ["prompt", "model"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let prompt = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'prompt'"))?;

    crate::tools::validate_input_length(prompt, "prompt")?;

    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'model'"))?;

    let allow_file_read = args
        .get("allow_file_read")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let allow_file_write = args
        .get("allow_file_write")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let max_retries = args
        .get("max_retries")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(3);

    let permissions = Permissions {
        allowed_tools: vec![], // Tool injection can be added here
        allow_file_read,
        allow_file_write,
    };

    let harness = AgentHarness::new(engine.provider.clone())
        .with_permissions(permissions)
        .with_retries(max_retries);

    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: prompt.to_string(),
        tool_calls: None,
        tool_call_id: None,
    }];

    match harness.chat_with_retry(&messages, model, None).await {
        Ok(response) => Ok(json!({
            "content": [{
                "type": "text",
                "text": response.message.content
            }],
            "isError": false
        })),
        Err(e) => Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("Harness execution failed: {}", e)
            }],
            "isError": true
        })),
    }
}
