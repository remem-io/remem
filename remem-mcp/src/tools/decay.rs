use crate::tools::ReasoningEngine;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn schema() -> Value {
    json!({
        "name": "mem_decay",
        "description": "Apply importance-weighted decay to all active memories and archive those that fall below the threshold.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "factor": {
                    "type": "number",
                    "description": "Decay factor (0.0 to 1.0, lower means faster decay). Defaults to 0.9.",
                    "minimum": 0.0,
                    "maximum": 1.0
                }
            }
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, arguments: &Value) -> anyhow::Result<Value> {
    let factor = arguments
        .get("factor")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.9);

    let archived_count = engine.apply_decay(factor).await?;

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": format!("✓ Applied memory decay (factor: {}). Archived {} memories.", factor, archived_count)
            }
        ],
        "archived_count": archived_count
    }))
}
