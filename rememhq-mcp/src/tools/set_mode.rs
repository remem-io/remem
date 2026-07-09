use rememhq_core::config::Mode;
use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_set_mode",
        "description": "Sets the memory engine's operational mode (standard, debugging, refactoring, exploration, writing) to tune recall limits and context token budgets.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["standard", "debugging", "refactoring", "exploration", "writing"],
                    "description": "The mode to set."
                }
            },
            "required": ["mode"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let mode_str = args
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'mode' argument"))?;

    let mode = match mode_str {
        "standard" => Mode::Standard,
        "debugging" => Mode::Debugging,
        "refactoring" => Mode::Refactoring,
        "exploration" => Mode::Exploration,
        "writing" => Mode::Writing,
        _ => return Err(anyhow::anyhow!("Invalid mode: {}", mode_str)),
    };

    *engine.mode.write().await = mode;

    Ok(serde_json::json!({
        "status": "success",
        "message": format!("Memory mode set to {:?}", mode)
    }))
}
