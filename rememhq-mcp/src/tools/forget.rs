//! mem_forget — delete, decay, or archive a memory.

use rememhq_core::memory::types::ForgetMode;
use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_forget",
        "description": "Delete a memory or reduce its decay score to trigger archival.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Memory UUID" },
                "mode": { "type": "string", "enum": ["delete", "decay", "archive"], "description": "Forget mode" }
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

    let mode: ForgetMode = args
        .get("mode")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(ForgetMode::Delete);

    let success = engine.forget(id, mode).await?;

    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": format!("{{ \"success\": {} }}", success) }]
    }))
}
