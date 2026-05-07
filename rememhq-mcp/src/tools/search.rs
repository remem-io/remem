//! mem_search — hybrid vector + keyword search without LLM re-ranking.

use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_search",
        "description": "Hybrid vector + keyword search without LLM re-ranking. Faster, lower cost, less accurate than mem_recall.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default 20)" },
                "filter_tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["query"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let filter_tags: Vec<String> = args
        .get("filter_tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let results = engine.search(query, limit, &filter_tags).await?;

    let text = serde_json::to_string_pretty(&results)?;
    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    }))
}
