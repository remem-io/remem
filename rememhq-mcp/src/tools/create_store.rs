use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_create_store",
        "description": "Create a new partitioned memory store.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Name of the new store" },
                "description": { "type": "string", "description": "Optional description for the store" }
            },
            "required": ["name"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing store name"))?;

    let description = args.get("description").and_then(|v| v.as_str());

    let store_id = engine.create_store(name, description).await?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&serde_json::json!({
                "status": "success",
                "store_id": store_id,
                "name": name,
                "description": description
            }))?
        }]
    }))
}
