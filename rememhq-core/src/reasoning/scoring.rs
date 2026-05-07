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
    // unused import removed

    // Unit test with a mock provider would go here
    // Integration tests with real APIs go in the evals/ directory
}
