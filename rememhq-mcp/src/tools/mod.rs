mod compact;
mod consolidate;
mod decay;
mod forget;
mod knowledge;
mod recall;
mod search;
mod store;
mod update;

mod create_store;
mod list_memories;
mod list_sessions;
mod log_action;
mod project_context;
mod build_context;
mod smart_read;
mod set_mode;
mod stats;

use rememhq_core::reasoning::ReasoningEngine;
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
        compact::schema(),
        knowledge::query_schema(),
        knowledge::entity_schema(),
        log_action::schema(),
        project_context::schema(),
        list_memories::schema(),
        list_sessions::schema(),
        stats::schema(),
        create_store::schema(),
        build_context::schema(),
        smart_read::schema(),
        set_mode::schema(),
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
        "mem_compact" => compact::handle(engine, &arguments).await,
        "mem_query_knowledge" => knowledge::handle_query(engine, &arguments).await,
        "mem_get_entity_context" => knowledge::handle_entity(engine, &arguments).await,
        "mem_log_action" => log_action::handle(engine, &arguments).await,
        "mem_get_project_context" => project_context::handle(engine, &arguments).await,
        "mem_build_context" => build_context::handle(engine, &arguments).await,
        "mem_smart_read" => smart_read::handle(engine, &arguments).await,
        "mem_set_mode" => set_mode::handle(engine, &arguments).await,
        "mem_list_memories" => list_memories::handle(engine, &arguments).await,
        "mem_list_sessions" => list_sessions::handle(engine, &arguments).await,
        "mem_stats" => stats::handle(engine, &arguments).await,
        "mem_create_store" => create_store::handle(engine, &arguments).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", tool_name)),
    }
}
