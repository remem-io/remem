use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::context::builder::ContextBuilder;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_build_context",
        "description": "Assembles a comprehensive context document for the project, within a specified token budget. This includes recent sessions, key facts, and knowledge graph data.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "token_budget": {
                    "type": "number",
                    "description": "Approximate maximum number of tokens to include in the context (default: 4000)"
                }
            },
            "required": []
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let mut token_budget = args.get("token_budget").and_then(|v| v.as_u64()).unwrap_or(4000) as usize;

    let current_mode = *engine.mode.read().await;
    token_budget = current_mode.adjust_token_budget(token_budget);

    let builder = ContextBuilder::new(engine, &engine.config.project, token_budget);
    let document = builder.build().await?;

    Ok(serde_json::json!({
        "status": "success",
        "context": document.to_string_formatted()
    }))
}
