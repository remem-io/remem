use rememhq_core::memory::types::SessionObservation;
use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_log_action",
        "description": "Log an observation or tool call to the current session transcript. This acts as an implicit memory that will be compressed into durable facts when the session ends.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The current session ID (e.g., 'session-123')."
                },
                "observation_type": {
                    "type": "string",
                    "description": "Type of observation: 'tool_call', 'prompt', 'result', etc."
                },
                "content": {
                    "type": "string",
                    "description": "The content to log."
                },
                "parent_id": {
                    "type": "string",
                    "description": "Optional ID of the parent observation to support session branching."
                }
            },
            "required": ["session_id", "observation_type", "content"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing session_id"))?;

    let observation_type = args
        .get("observation_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing observation_type"))?;

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing content"))?;

    let parent_id = match args.get("parent_id").and_then(|v| v.as_str()) {
        Some(id_str) => Some(
            uuid::Uuid::parse_str(id_str)
                .map_err(|_| anyhow::anyhow!("Invalid parent_id UUID format"))?,
        ),
        None => None,
    };

    let obs = SessionObservation::new(session_id, observation_type, content, parent_id);

    // Store observation in session_logs
    use rememhq_core::reasoning::ReasoningEvent;
    use rememhq_core::storage::MemoryStore;
    engine.store.log_session_observation(&obs).await?;

    if observation_type == "thinking" || observation_type == "prompt" {
        engine.emit_event(ReasoningEvent::ThinkingDelta {
            session_id: session_id.to_string(),
            thought: content.to_string(),
        });
    } else if observation_type == "tool_call" {
        engine.emit_event(ReasoningEvent::ToolCall {
            session_id: session_id.to_string(),
            tool_name: observation_type.to_string(),
            input_summary: content.to_string(),
        });
    } else {
        engine.emit_event(ReasoningEvent::ObservationStreamed {
            session_id: session_id.to_string(),
            observation_type: observation_type.to_string(),
            content: content.to_string(),
        });
    }

    Ok(serde_json::json!({
        "status": "success",
        "message": format!("Logged {} observation for session {}", observation_type, session_id)
    }))
}
