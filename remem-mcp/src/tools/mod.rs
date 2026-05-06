mod consolidate;
mod forget;
mod recall;
mod search;
mod store;
mod update;
mod decay;

use remem_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

/// Return the list of all MCP tools exposed by remem.
pub fn list_tools() -> Vec<Value> {
    vec![
        store::schema(),
        recall::schema(),
        search::schema(),
        update::schema(),
        forget::schema(),
        consolidate::schema(),
        decay::schema(),
    ]
}

/// Dispatch a tool call to the appropriate handler.
pub async fn call_tool(engine: &Arc<ReasoningEngine>, params: &Value) -> anyhow::Result<Value> {
    let tool_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    match tool_name {
        "mem_store" => store::handle(engine, &arguments).await,
        "mem_recall" => recall::handle(engine, &arguments).await,
        "mem_search" => search::handle(engine, &arguments).await,
        "mem_update" => update::handle(engine, &arguments).await,
        "mem_forget" => forget::handle(engine, &arguments).await,
        "mem_consolidate" => consolidate::handle(engine, &arguments).await,
        "mem_decay" => decay::handle(engine, &arguments).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
    }
}
