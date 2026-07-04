use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::MemoryStore;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_stats",
        "description": "Get statistics about the memory store.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, _args: &Value) -> anyhow::Result<Value> {
    let stats = engine.store.stats().await?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&serde_json::json!({
                "total_memories": stats.total_memories,
                "by_type": stats.by_type,
                "avg_importance": stats.avg_importance,
                "db_size_bytes": stats.db_size_bytes,
            }))?
        }]
    }))
}
