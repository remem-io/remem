//! Fine-Tuning Dataset Exporter.
//!
//! Converts memory consolidation, extraction, and contradiction decisions
//! into OpenAI / Anthropic format JSONL training datasets for model fine-tuning.

use crate::memory::types::{MemoryRecord, MemoryType};
use serde::{Deserialize, Serialize};

/// Role in a fine-tuning chat turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FineTuningRole {
    System,
    User,
    Assistant,
}

/// A message in a multi-turn fine-tuning conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningMessage {
    pub role: FineTuningRole,
    pub content: String,
}

/// A complete training sample formatted for standard LLM fine-tuning APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningSample {
    pub messages: Vec<FineTuningMessage>,
}

impl FineTuningSample {
    pub fn new(system: &str, user: &str, assistant: &str) -> Self {
        Self {
            messages: vec![
                FineTuningMessage {
                    role: FineTuningRole::System,
                    content: system.to_string(),
                },
                FineTuningMessage {
                    role: FineTuningRole::User,
                    content: user.to_string(),
                },
                FineTuningMessage {
                    role: FineTuningRole::Assistant,
                    content: assistant.to_string(),
                },
            ],
        }
    }
}

/// Exporter for converting memories and consolidation histories into JSONL datasets.
pub struct FineTuningExporter;

impl FineTuningExporter {
    /// Export memories of type Fact/Decision into a memory extraction fine-tuning dataset.
    pub fn export_memories(memories: &[MemoryRecord]) -> Vec<FineTuningSample> {
        let system_prompt = "You are a memory consolidation engine. Extract atomic durable facts, importance scores (1-10), and knowledge graph triples from conversation sessions.";
        let mut samples = Vec::new();

        for m in memories {
            if m.memory_type == MemoryType::Fact || m.memory_type == MemoryType::Decision {
                let user_input = format!("Session Observation:\n{}", m.content);
                let assistant_output = format!(
                    "FACT | {} | {} | {} | {}",
                    m.memory_type,
                    m.importance,
                    m.tags.join(", "),
                    m.content
                );
                samples.push(FineTuningSample::new(
                    system_prompt,
                    &user_input,
                    &assistant_output,
                ));
            }
        }

        samples
    }

    /// Serialize samples into standard newline-delimited JSON (JSONL).
    pub fn to_jsonl(samples: &[FineTuningSample]) -> String {
        samples
            .iter()
            .map(|s| serde_json::to_string(s).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fine_tuning_export_jsonl() {
        let mut mem = MemoryRecord::new("User uses Rust for backend development", MemoryType::Fact);
        mem.importance = 9.0;
        mem.tags = vec!["lang".into(), "backend".into()];

        let samples = FineTuningExporter::export_memories(&[mem]);
        assert_eq!(samples.len(), 1);

        let jsonl = FineTuningExporter::to_jsonl(&samples);
        assert!(jsonl.contains("User uses Rust"));
        assert!(jsonl.contains("\"role\":\"assistant\""));
    }
}
