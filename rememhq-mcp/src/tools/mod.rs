mod compact;
mod consolidate;
mod decay;
mod forget;
mod knowledge;
mod recall;
mod search;
mod store;
mod update;

mod build_context;
mod create_store;
mod list_memories;
mod list_sessions;
mod log_action;
mod project_context;
mod set_mode;
mod smart_read;
mod stats;

use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

/// Upper bound on `limit` accepted by MCP tools that take one directly from
/// tool-call arguments — which, unlike a human typing into a REST API, may
/// be set by an LLM agent acting on untrusted content (e.g. prompt
/// injection). Without a cap, `limit` is passed straight through to the
/// vector index search and, downstream, an FFI call into the native HNSW
/// library, which can force a huge allocation/search. This mirrors
/// `MAX_FETCH_LIMIT` in `rememhq-api/src/routes/memories.rs`, fixed for the
/// same reason on the REST `/v1/memories/recall` and `/v1/memories/search`
/// endpoints.
pub(crate) const MAX_TOOL_LIMIT: usize = 1000;

/// Clamp a `limit` parsed from tool-call arguments to [`MAX_TOOL_LIMIT`].
pub(crate) fn clamp_limit(limit: usize) -> usize {
    limit.min(MAX_TOOL_LIMIT)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_limit() {
        assert_eq!(clamp_limit(8), 8, "values under the cap pass through unchanged");
        assert_eq!(clamp_limit(1000), 1000, "the cap itself is allowed");
        assert_eq!(clamp_limit(1001), MAX_TOOL_LIMIT);
        assert_eq!(clamp_limit(usize::MAX), MAX_TOOL_LIMIT);
    }
}
