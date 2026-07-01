//! mem_consolidate — trigger reasoning consolidation over a session.

use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_consolidate",
        "description": "Trigger a reasoning consolidation pass over a session's working memory. Extracts durable facts, scores importance, detects contradictions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session ID to consolidate" },
                "model": { "type": "string", "description": "Override reasoning model" },
                "api_key": { "type": "string", "description": "Optional API key for dynamic configuration" }
            },
            "required": ["session_id"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing session_id"))?;

    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&engine.config.reasoning.reasoning_model)
        .to_string();

    let api_key = args
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
        
    let options = api_key.map(|key| rememhq_core::providers::ProviderOptions {
        api_key: Some(key),
        ..Default::default()
    });

    let report = rememhq_core::reasoning::consolidation::consolidate_session(
        &*engine.provider,
        &*engine.embeddings,
        &engine.store,
        engine.index.as_ref(),
        session_id,
        &model,
        options.as_ref(),
    )
    .await?;

    let text = serde_json::to_string_pretty(&report)?;
    Ok(serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    }))
}
