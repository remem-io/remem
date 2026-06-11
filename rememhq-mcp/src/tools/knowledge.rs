//! mem_query_knowledge — query the knowledge graph for subject/predicate/object triples.
//! mem_get_entity_context — retrieve all triples involving a named entity.

use rememhq_core::reasoning::ReasoningEngine;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn query_schema() -> Value {
    json!({
        "name": "mem_query_knowledge",
        "description": "Query the knowledge graph. Returns subject-predicate-object triples \
                        matching the given filters. All filters are optional — omitting them \
                        returns everything.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "Filter by subject entity name"
                },
                "predicate": {
                    "type": "string",
                    "description": "Filter by relationship type (e.g. 'lives_in', 'next_step')"
                },
                "object": {
                    "type": "string",
                    "description": "Filter by object entity name"
                }
            }
        }
    })
}

pub fn entity_schema() -> Value {
    json!({
        "name": "mem_get_entity_context",
        "description": "Retrieve all knowledge graph triples where the given entity appears \
                        as either the subject or the object. Useful for getting everything \
                        known about a person, system, or concept.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "entity": {
                    "type": "string",
                    "description": "Entity name to look up"
                }
            },
            "required": ["entity"]
        }
    })
}

pub async fn handle_query(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let subject = args.get("subject").and_then(|v| v.as_str());
    let predicate = args.get("predicate").and_then(|v| v.as_str());
    let object = args.get("object").and_then(|v| v.as_str());

    let triples = engine.query_knowledge(subject, predicate, object).await?;

    let text = if triples.is_empty() {
        "No matching knowledge graph triples found.".to_string()
    } else {
        serde_json::to_string_pretty(&triples)?
    };

    Ok(json!({ "content": [{ "type": "text", "text": text }] }))
}

pub async fn handle_entity(engine: &Arc<ReasoningEngine>, args: &Value) -> anyhow::Result<Value> {
    let entity = args
        .get("entity")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required field: entity"))?;

    let triples = engine.get_entity_context(entity).await?;

    let text = if triples.is_empty() {
        format!("No knowledge graph entries found for entity: {entity}")
    } else {
        serde_json::to_string_pretty(&triples)?
    };

    Ok(json!({ "content": [{ "type": "text", "text": text }] }))
}
