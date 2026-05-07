//! mem_store — store a new memory with automatic LLM importance scoring.

use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_store",
        "description": "Store a new memory. The LLM scores importance automatically if not provided.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "The fact or insight to remember" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Classification tags" },
                "importance": { "type": "number", "description": "Importance score 1-10 (auto-scored if omitted)" },
                "ttl_days": { "type": "integer", "description": "Time-to-live in days (null = permanent)" },
                "type": { "type": "string", "enum": ["fact", "procedure", "preference", "decision"], "description": "Memory type" }
            },
            "required": ["content"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing content"))?;

    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let importance = args
        .get("importance")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    let ttl_days = args
        .get("ttl_days")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let memory_type: MemoryType = args
        .get("type")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(MemoryType::Fact);

    let mut record = MemoryRecord::new(content, memory_type).with_tags(tags);
    if let Some(ttl) = ttl_days {
        record = record.with_ttl(ttl);
    }

    let auto_score = importance.is_none();
    if let Some(imp) = importance {
        record = record.with_importance(imp);
    }

    let stored = engine.store_memory(record, auto_score).await?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&serde_json::json!({
                "id": stored.id,
                "importance": stored.importance,
                "tags": stored.tags,
                "created_at": stored.created_at,
            }))?
        }]
    }))
}
