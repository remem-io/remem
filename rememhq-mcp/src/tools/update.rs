//! mem_update — update an existing memory's content, importance, or tags.

use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_update",
        "description": "Update an existing memory's content, importance, or tags.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Memory UUID" },
                "content": { "type": "string", "description": "New content" },
                "importance": { "type": "number", "description": "New importance score" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "New tags" }
            },
            "required": ["id"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let id_str = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
    let id = uuid::Uuid::parse_str(id_str)?;

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .map(String::from);
    let importance = args
        .get("importance")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let tags: Option<Vec<String>> = args
        .get("tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let updated = engine.update_memory(id, content, importance, tags).await?;

    let text = serde_json::to_string_pretty(&serde_json::json!({
        "id": updated.id,
        "content": updated.content,
        "importance": updated.importance,
        "tags": updated.tags,
        "updated_at": updated.updated_at,
    }))?;

    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    }))
}
