//! Importance scoring — LLM-based rating of memory importance (1-10).

use crate::providers::{Provider, ProviderOptions};

/// Score the importance of a memory using an LLM.
///
/// The LLM is prompted to rate the memory on a 1-10 scale based on
/// how durable, actionable, and universally relevant the information is.
pub async fn score_importance(
    provider: &dyn Provider,
    content: &str,
    model: &str,
    options: Option<&ProviderOptions>,
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

    let (response, _usage) = provider.complete(&prompt, model, options).await?;

    // Parse the numeric response. Models asked to "respond with ONLY a number"
    // still sometimes wrap it in text (e.g. "Score: 7", "7/10", "I'd say 7."),
    // so fall back to scanning for the first numeric token before giving up.
    let trimmed = response.trim();
    let score = trimmed
        .parse::<f32>()
        .ok()
        .or_else(|| extract_first_number(trimmed))
        .unwrap_or_else(|| {
            tracing::warn!(
                response = %trimmed,
                "score_importance: could not parse a number from LLM response, defaulting to 5.0"
            );
            5.0
        })
        .clamp(1.0, 10.0);

    Ok(score)
}

/// Scans `text` for the first substring that looks like a number (optionally
/// signed, optionally with a single decimal point) and parses it.
///
/// Used as a fallback when the LLM doesn't strictly follow a "respond with
/// ONLY a number" instruction and wraps the number in surrounding text.
fn extract_first_number(text: &str) -> Option<f32> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = if i > 0 && chars[i - 1] == '-' {
                i - 1
            } else {
                i
            };
            let mut end = i;
            let mut seen_dot = false;
            while end < chars.len()
                && (chars[end].is_ascii_digit() || (chars[end] == '.' && !seen_dot))
            {
                if chars[end] == '.' {
                    seen_dot = true;
                }
                end += 1;
            }
            let token: String = chars[start..end].iter().collect();
            if let Ok(n) = token.parse::<f32>() {
                return Some(n);
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockProvider;

    #[tokio::test]
    async fn test_score_importance_returns_valid_range() {
        let provider = MockProvider;
        // MockProvider returns "Mock response" which is non-numeric, so defaults to 5.0
        let score = score_importance(&provider, "Alice uses Rust for backend", "mock", None)
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
            async fn complete(
                &self,
                _prompt: &str,
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
                Ok(("15".to_string(), None))
            }
            async fn chat(
                &self,
                _messages: &[crate::providers::ChatMessage],
                _tools: &[crate::providers::Tool],
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "high_score_mock"
            }
        }

        let provider = HighScoreProvider;
        let score = score_importance(&provider, "Critical secret key", "mock", None)
            .await
            .unwrap();
        assert_eq!(score, 10.0, "Scores above 10 should be clamped to 10.0");
    }

    #[tokio::test]
    async fn test_score_importance_clamps_low_value() {
        struct LowScoreProvider;
        #[async_trait::async_trait]
        impl Provider for LowScoreProvider {
            async fn complete(
                &self,
                _prompt: &str,
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
                Ok(("-3".to_string(), None))
            }
            async fn chat(
                &self,
                _messages: &[crate::providers::ChatMessage],
                _tools: &[crate::providers::Tool],
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "low_score_mock"
            }
        }

        let provider = LowScoreProvider;
        let score = score_importance(&provider, "trivial info", "mock", None)
            .await
            .unwrap();
        assert_eq!(score, 1.0, "Scores below 1 should be clamped to 1.0");
    }

    #[tokio::test]
    async fn test_score_importance_parses_valid_number() {
        struct ExactScoreProvider;
        #[async_trait::async_trait]
        impl Provider for ExactScoreProvider {
            async fn complete(
                &self,
                _prompt: &str,
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
                Ok(("  7  \n".to_string(), None)) // whitespace + newline
            }
            async fn chat(
                &self,
                _messages: &[crate::providers::ChatMessage],
                _tools: &[crate::providers::Tool],
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "exact_score_mock"
            }
        }

        let provider = ExactScoreProvider;
        let score = score_importance(&provider, "important config detail", "mock", None)
            .await
            .unwrap();
        assert_eq!(score, 7.0, "Should parse '  7  \\n' as 7.0");
    }

    #[tokio::test]
    async fn test_score_importance_extracts_number_from_wrapped_text() {
        struct WrappedScoreProvider;
        #[async_trait::async_trait]
        impl Provider for WrappedScoreProvider {
            async fn complete(
                &self,
                _prompt: &str,
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<(String, Option<crate::providers::TokenUsage>)> {
                Ok(("Score: 7/10".to_string(), None))
            }
            async fn chat(
                &self,
                _messages: &[crate::providers::ChatMessage],
                _tools: &[crate::providers::Tool],
                _model: &str,
                _options: Option<&ProviderOptions>,
            ) -> anyhow::Result<crate::providers::ChatResponse> {
                Err(anyhow::anyhow!("mock chat not supported"))
            }
            fn name(&self) -> &str {
                "wrapped_score_mock"
            }
        }

        let provider = WrappedScoreProvider;
        let score = score_importance(&provider, "important config detail", "mock", None)
            .await
            .unwrap();
        assert_eq!(
            score, 7.0,
            "Should extract 7.0 from a response that doesn't strictly follow the 'ONLY a number' instruction"
        );
    }

    #[test]
    fn test_extract_first_number() {
        assert_eq!(extract_first_number("7"), Some(7.0));
        assert_eq!(extract_first_number("Score: 7"), Some(7.0));
        assert_eq!(extract_first_number("7/10"), Some(7.0));
        assert_eq!(
            extract_first_number("I'd rate this a 7.5 out of 10"),
            Some(7.5)
        );
        assert_eq!(extract_first_number("-3 (too trivial)"), Some(-3.0));
        assert_eq!(extract_first_number("no numbers here"), None);
    }
}
