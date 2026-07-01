use crate::providers::{EmbeddingProvider, Provider, ProviderOptions};
use async_trait::async_trait;

pub struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn complete(&self, prompt: &str, _model: &str, _options: Option<&ProviderOptions>) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
        if prompt.contains("contradiction detector") {
            if prompt.contains("New York") && prompt.contains("London") {
                return Ok(("CONTRADICTION | [CANDIDATE-1] | Alice moved to New York, so London is outdated.".to_string(), None));
            }
            return Ok(("NONE".to_string(), None));
        }
        if prompt.contains("FACT_EXTRACTION") || prompt.contains("Output the facts now") {
            if prompt.contains("To bake a cake") {
                return Ok((r#"FACT | procedure | 7 | baking | First, preheat the oven
TRIPLE | First, preheat the oven | next_step | Then, mix the batter
FACT | procedure | 7 | baking | Then, mix the batter"#
                    .to_string(), None));
            }
            if prompt.contains("New York") {
                return Ok(("FACT | fact | 9 | relocation | Alice moved to New York".to_string(), None));
            }
            return Ok((r#"FACT | fact | 8 | rust | Alice likes Rust"#.to_string(), None));
        }
        if prompt.contains("entity resolution engine") {
            if prompt.contains("Postgres") && prompt.contains("PostgreSQL") {
                return Ok(("PostgreSQL".to_string(), None));
            }
            if prompt.contains("New Entity: \"Port 5432\"") {
                return Ok(("Port 5432".to_string(), None));
            }
        }
        Ok(("Mock response".to_string(), None))
    }

    async fn chat(
        &self,
        _messages: &[super::ChatMessage],
        _tools: &[super::Tool],
        _model: &str,
        _options: Option<&ProviderOptions>,
    ) -> anyhow::Result<super::ChatResponse> {
        Err(anyhow::anyhow!("MockProvider does not support chat API"))
    }

    fn name(&self) -> &str {
        "mock"
    }
}

pub struct MockEmbeddings {
    dim: usize,
}

impl MockEmbeddings {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddings {
    async fn embed(&self, text: &str, _options: Option<&ProviderOptions>) -> anyhow::Result<Vec<f32>> {
        let mut vec = vec![0.0; self.dim];
        if !text.is_empty() {
            if text.contains("Alice") {
                vec[0] = 1.0;
            } else {
                let sum: u32 = text.chars().map(|c| c as u32).sum();
                // Normalize slightly to ensure deterministic differing values
                vec[0] = ((sum % 100) as f32 + 1.0) / 101.0;
                vec[1] = ((sum % 7) as f32 + 1.0) / 8.0;
            }
        }
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String], _options: Option<&ProviderOptions>) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::new();
        for t in texts {
            results.push(self.embed(t, _options).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}
