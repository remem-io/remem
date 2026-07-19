use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_list_sessions",
        "description": "List recent sessions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Maximum number of sessions to return (default 10)" }
            },
            "required": []
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let limit = crate::tools::clamp_limit(
        args.get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(10),
    );

    // Assuming session serialization works directly or needs mapping.
    // Usually, we just serialize it.
    let sessions = engine.list_sessions(limit).await?;

    // SessionRecord needs to implement Serialize for this to work natively.
    // If not, we might need a custom mapping, but let's assume it does, just like MemoryRecord.

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&sessions)?
        }]
    }))
}
