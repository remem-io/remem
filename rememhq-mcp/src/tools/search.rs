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
                "filter_tags": { "type": "array", "items": { "type": "string" } },
                "api_key": { "type": "string", "description": "Optional API key for dynamic configuration" }
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

    let limit = crate::tools::clamp_limit(args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize);

    let filter_tags: Vec<String> = args
        .get("filter_tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let api_key = args
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let options =
        api_key.map(|key| rememhq_core::providers::ProviderOptions { api_key: Some(key) });

    let results = engine
        .search(query, limit, &filter_tags, options.as_ref())
        .await?;

    let text = serde_json::to_string_pretty(&results)?;
    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    }))
}
