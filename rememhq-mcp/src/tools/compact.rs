use rememhq_core::reasoning::ReasoningEngine;
use serde_json::Value;
use std::sync::Arc;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_compact",
        "description": "Compact a conversation trace to save context window tokens while preserving critical state and architectural decisions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "conversation_text": {
                    "type": "string",
                    "description": "The raw conversation log or JSON trace to be compressed."
                },
                "focus_areas": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional specific topics to ensure are preserved in the compaction."
                },
                "api_key": {
                    "type": "string",
                    "description": "Optional API key for dynamic configuration"
                }
            },
            "required": ["conversation_text"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, arguments: &Value) -> anyhow::Result<Value> {
    let conversation_text = arguments
        .get("conversation_text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'conversation_text' parameter"))?;

    let focus_areas: Option<Vec<String>> = arguments
        .get("focus_areas")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect()
        });

    let api_key = arguments
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
        
    let options = api_key.map(|key| rememhq_core::providers::ProviderOptions {
        api_key: Some(key),
        ..Default::default()
    });

    let report = engine
        .compact_context(conversation_text, focus_areas.as_deref(), options.as_ref())
        .await?;

    Ok(serde_json::json!({
        "content": [{
            "type": "text",
            "text": format!(
                "Context successfully compacted (Original: {} chars, Compacted: {} chars).\n\nCompacted Context:\n{}",
                report.original_length, report.compressed_length, report.compressed_context
            )
        }]
    }))
}
