use rememhq_core::reasoning::ReasoningEngine;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn schema() -> Value {
    json!({
        "name": "mem_run_triage_graph",
        "description": "Run the CI Triage graph on a set of CI logs.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "ci_logs": {
                    "type": "string",
                    "description": "The raw CI logs to triage."
                },
                "model": {
                    "type": "string",
                    "description": "The reasoning model to use."
                }
            },
            "required": ["ci_logs", "model"]
        }
    })
}

pub async fn handle(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let ci_logs = args
        .get("ci_logs")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'ci_logs'"))?;

    crate::tools::validate_input_length(ci_logs, "ci_logs")?;

    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing 'model'"))?;

    let triage_state = rememhq_core::reasoning::triage::run_triage_graph(
        engine.provider.clone(),
        model,
        ci_logs.to_string(),
    )
    .await?;

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Triage finished.\nFailure Type: {:?}\nResolved: {}\nFix Attempt: {:?}\nEscalation Reason: {:?}",
                triage_state.failure_type,
                triage_state.is_resolved,
                triage_state.fix_attempt,
                triage_state.escalation_reason)
        }],
        "isError": false
    }))
}
