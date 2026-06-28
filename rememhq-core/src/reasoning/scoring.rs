//! Importance scoring — LLM-based rating of memory importance (1-10).

use crate::providers::Provider;

/// Score the importance of a memory using an LLM.
///
/// The LLM is prompted to rate the memory on a 1-10 scale based on
/// how durable, actionable, and universally relevant the information is.
pub async fn score_importance(
    provider: &dyn Provider,
    content: &str,
    model: &str,
) -> anyhow::Result<f32> {
    let prompt = format!(
        r#"Rate the importance of this piece of information on a scale of 1-10 for an AI agent's long-term memory.

Scoring criteria:
- 9-10: Critical system facts, security credentials, core architecture decisions
- 7-8: Important preferences, workflow patterns, key technical details
- 5-6: Useful context, moderate-term relevant information
- 3-4: Minor details, short-term relevant information
- 1-2: Trivial, highly ephemeral information

Information to rate:
"{content}"

Respond with ONLY a single number between 1 and 10, nothing else."#
    );

    let response = provider.complete(&prompt, model).await?;

    // Parse the numeric response
    let score = response
        .trim()
        .parse::<f32>()
        .unwrap_or(5.0)
        .clamp(1.0, 10.0);

    Ok(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockProvider;

    #[tokio::test]
    async fn test_score_importance_returns_valid_range() {
        let provider = MockProvider;
        // MockProvider returns "Mock response" which is non-numeric, so defaults to 5.0
        let score = score_importance(&provider, "Alice uses Rust for backend", "mock")
            .await
            .unwrap();
        assert!(
            (1.0..=10.0).contains(&score),
            "Score {} should be between 1.0 and 10.0",
            score
        );
        assert_eq!(
            score, 5.0,
            "Non-numeric mock response should default to 5.0"
        );
    }

    #[tokio::test]
    async fn test_score_importance_clamps_high_value() {
        // Create a provider that returns a value above 10
        struct HighScoreProvider;
        #[async_trait::async_trait]
        impl Provider for HighScoreProvider {
            async fn complete(&self, _prompt: &str, _model: &str) -> anyhow::Result<String> {
                Ok("15".to_string())
            }
            async fn chat(&self, _messages: &[crate::providers::ChatMessage], _tools: &[crate::providers::Tool], _model: &str) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "high_score_mock"
            }
        }

        let provider = HighScoreProvider;
        let score = score_importance(&provider, "Critical secret key", "mock")
            .await
            .unwrap();
        assert_eq!(score, 10.0, "Scores above 10 should be clamped to 10.0");
    }

    #[tokio::test]
    async fn test_score_importance_clamps_low_value() {
        struct LowScoreProvider;
        #[async_trait::async_trait]
        impl Provider for LowScoreProvider {
            async fn complete(&self, _prompt: &str, _model: &str) -> anyhow::Result<String> {
                Ok("-3".to_string())
            }
            async fn chat(&self, _messages: &[crate::providers::ChatMessage], _tools: &[crate::providers::Tool], _model: &str) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "low_score_mock"
            }
        }

        let provider = LowScoreProvider;
        let score = score_importance(&provider, "trivial info", "mock")
            .await
            .unwrap();
        assert_eq!(score, 1.0, "Scores below 1 should be clamped to 1.0");
    }

    #[tokio::test]
    async fn test_score_importance_parses_valid_number() {
        struct ExactScoreProvider;
        #[async_trait::async_trait]
        impl Provider for ExactScoreProvider {
            async fn complete(&self, _prompt: &str, _model: &str) -> anyhow::Result<String> {
                Ok("  7  \n".to_string()) // whitespace + newline
            }
            async fn chat(&self, _messages: &[crate::providers::ChatMessage], _tools: &[crate::providers::Tool], _model: &str) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "exact_score_mock"
            }
        }

        let provider = ExactScoreProvider;
        let score = score_importance(&provider, "important config detail", "mock")
            .await
            .unwrap();
        assert_eq!(score, 7.0, "Should parse '  7  \\n' as 7.0");
    }
}
