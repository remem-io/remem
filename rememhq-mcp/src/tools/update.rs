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
                "tags": { "type": "array", "items": { "type": "string" }, "description": "New tags" },
                "api_key": { "type": "string", "description": "Optional API key for dynamic configuration" }
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

    let api_key = args
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let options = api_key.map(|key| rememhq_core::providers::ProviderOptions {
        api_key: Some(key),
    });

    let updated = engine
        .update_memory(id, content, importance, tags, options.as_ref())
        .await?;

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
