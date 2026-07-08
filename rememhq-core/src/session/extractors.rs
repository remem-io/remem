use crate::memory::types::SessionObservation;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use uuid::Uuid;

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

                // Convert whole json object as content if no "content" field is present
                let content = value
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| value.to_string());

                let obs = SessionObservation::new(
                    session_id,
                    obs_type,
                    content,
                    None, // parent_id
                );
                observations.push(obs);
            }
        }

        Ok(observations)
    }
}
