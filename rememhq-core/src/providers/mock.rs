use crate::providers::{EmbeddingProvider, Provider};
use async_trait::async_trait;

pub struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn complete(&self, prompt: &str, _model: &str) -> anyhow::Result<String> {
        if prompt.contains("contradiction detector") {
            if prompt.contains("New York") && prompt.contains("London") {
                return Ok("CONTRADICTION | [CANDIDATE-1] | Alice moved to New York, so London is outdated.".to_string());
            }
            return Ok("NONE".to_string());
        }
        if prompt.contains("FACT_EXTRACTION") || prompt.contains("Output the facts now") {
            if prompt.contains("To bake a cake") {
                return Ok(r#"FACT | procedure | 7 | baking | First, preheat the oven
TRIPLE | First, preheat the oven | next_step | Then, mix the batter
FACT | procedure | 7 | baking | Then, mix the batter"#
                    .to_string());
            }
            return Ok(r#"FACT | fact | 8 | rust | Alice likes Rust"#.to_string());
        }
        if prompt.contains("entity resolution engine") {
            if prompt.contains("Postgres") && prompt.contains("PostgreSQL") {
                return Ok("PostgreSQL".to_string());
            }
            if prompt.contains("New Entity: \"Port 5432\"") {
                return Ok("Port 5432".to_string());
            }
        }
        Ok("Mock response".to_string())
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
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        let mut vec = vec![0.0; self.dim];
        if !_text.is_empty() {
            vec[0] = 1.0; // Very basic stable embedding
        }
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::new();
        for t in texts {
            results.push(self.embed(t).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}
