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
            },
            "required": []
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let limit = crate::tools::clamp_limit(args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize);
    use rememhq_core::storage::MemoryStore;
    let memories = engine.store.list(&[], None, None, limit).await?;

    let mut context_summary = String::new();
    context_summary.push_str("Here is the relevant project context and memories:\n\n");

    if memories.is_empty() {
        context_summary.push_str("No memories found for this project yet.\n\n");
    } else {
        for (i, mem) in memories.into_iter().enumerate() {
            context_summary.push_str(&format!(
                "{}. [{}] {}\n",
                i + 1,
                mem.memory_type,
                mem.content
            ));
        }
        context_summary.push('\n');
    }

    // Fetch and append recent session summaries
    let recent_sessions = engine
        .store
        .get_recent_session_summaries(&engine.config.project, 5)
        .await
        .unwrap_or_default();

    if !recent_sessions.is_empty() {
        context_summary.push_str("### Recent Sessions Timeline\n\n");
        for session in recent_sessions {
            context_summary.push_str(&format!(
                "- [{}]: {}\n",
                session.timestamp.format("%Y-%m-%d %H:%M"),
                session.summary
            ));
        }
    }

    Ok(serde_json::json!({
        "status": "success",
        "project_context": context_summary
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_validity() {
        let s = schema();
        assert_eq!(s["name"], "mem_get_project_context");
        assert!(s["description"].is_string());
        assert_eq!(s["inputSchema"]["type"], "object");
        assert!(s["inputSchema"]["properties"].is_object());
        assert!(s["inputSchema"]["required"].is_array());
    }
}
