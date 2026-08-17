use crate::memory::types::SessionObservation;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub struct TranscriptExtractor;

impl TranscriptExtractor {
    pub fn extract_from_file(
        path: &Path,
        session_id: &str,
    ) -> anyhow::Result<Vec<SessionObservation>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut observations = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            // Minimal JSONL parsing
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                let obs_type = value
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let mut content = String::new();

                // Handle antigravity-cli specific format
                if let Some(_ag_type) = value.get("type").and_then(|v| v.as_str()) {
                    if let Some(c) = value.get("content").and_then(|v| v.as_str()) {
                        content.push_str(c);
                    }
                    if let Some(tool_calls) = value.get("tool_calls").and_then(|v| v.as_array()) {
                        if !tool_calls.is_empty() {
                            content.push_str("\nTool Calls:\n");
                            for t in tool_calls {
                                let tool = t.as_str().unwrap_or("unknown");
                                content.push_str(&format!("- {}\n", tool));
                            }
                        }
                    }
                }

                if content.is_empty() {
                    // Convert whole json object as content if no "content" field is present
                    content = value
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| value.to_string());
                }

                let obs = SessionObservation::new(
                    session_id, obs_type, content, None, // parent_id
                );
                observations.push(obs);
            }
        }

        Ok(observations)
    }
}
