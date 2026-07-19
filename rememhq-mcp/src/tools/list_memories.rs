use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_list_memories",
        "description": "List memories, optionally filtered by store.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Maximum number of memories to return (default 50)" },
                "store_id": { "type": "string", "description": "Optional store ID to list memories from a specific store" }
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
            .unwrap_or(50),
    );

    let store_id = args.get("store_id").and_then(|v| v.as_str());

    let memories = if let Some(sid) = store_id {
        engine.list_memories_by_store(sid).await?
    } else {
        engine.list_memories(&[], None, None, limit).await?
    };

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&memories)?
        }]
    }))
}
