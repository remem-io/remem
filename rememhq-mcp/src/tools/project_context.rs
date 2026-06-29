use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_get_project_context",
        "description": "Fetch a compressed summary of the project's historical context, conventions, and key decisions. Use this when starting a new session or encountering a new codebase to get up to speed.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {
                    "type": "number",
                    "description": "Max number of facts to retrieve (default: 20)"
                }
            }
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    use rememhq_core::storage::MemoryStore;
    let memories = engine.store.list(&[], None, None, limit).await?;

    let mut context_summary = String::new();
    context_summary.push_str("Here is the relevant project context and memories:\n\n");

    for (i, mem) in memories.iter().enumerate() {
        context_summary.push_str(&format!(
            "{}. [{}] {}\n",
            i + 1,
            mem.memory_type,
            mem.content
        ));
    }

    if memories.is_empty() {
        context_summary.push_str("No memories found for this project yet.");
    }

    Ok(serde_json::json!({
        "status": "success",
        "project_context": context_summary
    }))
}
