use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::context::SmartReader;
use serde_json::Value;
use std::sync::Arc;
use std::path::Path;

pub fn schema() -> Value {
    serde_json::json!({
        "name": "mem_smart_read",
        "description": "Smartly reads a file, folding irrelevant code blocks to save context window space.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read."
                },
                "query": {
                    "type": "string",
                    "description": "Optional query to determine what is relevant."
                }
            },
            "required": ["path"]
        }
    })
}

pub async fn handle(_engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

    let query = args.get("query").and_then(|v| v.as_str());

    let content = SmartReader::read_and_fold(Path::new(path_str), query)?;

    Ok(serde_json::json!({
        "status": "success",
        "content": content
    }))
}
