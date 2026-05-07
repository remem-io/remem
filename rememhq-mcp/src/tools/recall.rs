//! mem_recall — guided retrieval with LLM re-ranking and reasoning traces.

use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_recall",
        "description": "Guided recall. Runs vector search then LLM re-ranking. Returns memories most relevant to the query with a reasoning trace.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to recall" },
                "limit": { "type": "integer", "description": "Max results (default 8)" },
                "filter_tags": { "type": "array", "items": { "type": "string" }, "description": "Filter by tags" },
                "since": { "type": "string", "description": "ISO 8601 date filter" },
                "memory_type": { "type": "string", "enum": ["fact", "procedure", "preference", "decision"] }
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

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8) as usize;

    let filter_tags: Vec<String> = args
        .get("filter_tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let since = args
        .get("since")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let memory_type = args
        .get("memory_type")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    let results = engine
        .recall(query, limit, &filter_tags, since, memory_type)
        .await?;

    let text = serde_json::to_string_pretty(&results)?;
    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    }))
}
