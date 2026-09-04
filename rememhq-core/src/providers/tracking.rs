//! Cost-tracking `Provider` decorator.
//!
//! `CostTracker::record_usage()` (see `pool.rs`) existed but nothing
//! called it: every reasoning call site (`reasoning/scoring.rs`,
//! `consolidation.rs`, `contradiction.rs`, `expansion.rs`, `compaction.rs`,
//! `resolution.rs`, `retrieval.rs`) destructures a provider response as
//! `let (response, _usage) = provider.complete(...).await?` and discards
//! the usage — so `CostSummary`/the Telemetry & Cost Dashboard's cost
//! panels were correct-but-inert.
//!
//! Rather than threading a `&CostTracker` parameter through each of those
//! (and their harness/eval-loop callers, and every test mock that
//! constructs them — a wide, easy-to-partially-miss change), this wraps
//! the *provider itself* once, where it's constructed
//! (`ReasoningEngine::new()`), with a decorator that records usage after
//! every call and otherwise passes everything through unchanged. Every
//! reasoning function above takes `provider: &dyn Provider` already, and
//! `AgentHarness`/the eval loop get their provider by cloning
//! `engine.provider` — so wrapping once at the source covers all of them
//! for free, with no changes to any of those files.

use async_trait::async_trait;
use std::sync::Arc;

use super::pool::CostTracker;
use super::{ChatMessage, ChatResponse, Provider, ProviderOptions, Tool, TokenUsage};

/// Wraps an inner [`Provider`], recording token usage and estimated cost
/// into a shared [`CostTracker`] after every successful call. Otherwise
/// fully transparent: responses (and errors) pass through unchanged, and
/// [`Provider::name`] delegates to the inner provider so callers/logs
/// still see e.g. `"anthropic"` or `"local"`, not `"cost_tracking"`.
pub struct CostTrackingProvider {
    inner: Arc<dyn Provider>,
    cost_tracker: Arc<CostTracker>,
}

impl CostTrackingProvider {
    pub fn new(inner: Arc<dyn Provider>, cost_tracker: Arc<CostTracker>) -> Self {
        Self {
            inner,
            cost_tracker,
        }
    }

    /// Record `usage` if present. A call that errored (and so never
    /// produced a response, let alone usage) or that succeeded without a
    /// `usage` block (some local runtimes omit it — see
    /// `providers/local.rs`) records nothing; there's nothing accurate to
    /// record in either case.
    fn record(&self, model: &str, usage: &Option<TokenUsage>) {
        if let Some(u) = usage {
            self.cost_tracker.record_usage(
                self.inner.name(),
                model,
                u.prompt_tokens,
                u.completion_tokens,
            );
        }
    }
}

#[async_trait]
impl Provider for CostTrackingProvider {
    async fn complete(
        &self,
        prompt: &str,
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<(String, Option<TokenUsage>)> {
        let result = self.inner.complete(prompt, model, options).await?;
        self.record(model, &result.1);
        Ok(result)
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[Tool],
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<ChatResponse> {
        let response = self.inner.chat(messages, tools, model, options).await?;
        self.record(model, &response.usage);
        Ok(response)
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatRole;

    /// Returns a fixed `TokenUsage` from both `complete()` and `chat()`,
    /// so tests can assert the wrapper actually recorded it.
    struct FixedUsageProvider {
        prompt_tokens: usize,
        completion_tokens: usize,
    }

    #[async_trait]
    impl Provider for FixedUsageProvider {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<TokenUsage>)> {
            Ok((
                "response".to_string(),
                Some(TokenUsage {
                    prompt_tokens: self.prompt_tokens,
                    completion_tokens: self.completion_tokens,
                    total_tokens: self.prompt_tokens + self.completion_tokens,
                }),
            ))
        }

        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                message: ChatMessage {
                    role: ChatRole::Assistant,
                    content: "response".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                },
                usage: Some(TokenUsage {
                    prompt_tokens: self.prompt_tokens,
                    completion_tokens: self.completion_tokens,
                    total_tokens: self.prompt_tokens + self.completion_tokens,
                }),
            })
        }

        fn name(&self) -> &str {
            "fixed_usage_mock"
        }
    }

    /// Always errors, never producing a response or usage — used to check
    /// the wrapper doesn't record anything for a failed call.
    struct AlwaysErrorsProvider;

    #[async_trait]
    impl Provider for AlwaysErrorsProvider {
        async fn complete(
            &self,
            _prompt: &str,
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<(String, Option<TokenUsage>)> {
            Err(anyhow::anyhow!("simulated provider failure"))
        }

        async fn chat(
            &self,
            _messages: &[ChatMessage],
            _tools: &[Tool],
            _model: &str,
            _options: Option<&ProviderOptions>,
        ) -> anyhow::Result<ChatResponse> {
            Err(anyhow::anyhow!("simulated provider failure"))
        }

        fn name(&self) -> &str {
            "always_errors_mock"
        }
    }

    #[tokio::test]
    async fn test_complete_records_usage_and_passes_response_through() {
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 100,
            completion_tokens: 50,
        });
        let tracker = Arc::new(CostTracker::new());
        let wrapped = CostTrackingProvider::new(inner, tracker.clone());

        let (text, usage) = wrapped.complete("prompt", "phi-3-mini", None).await.unwrap();
        assert_eq!(text, "response");
        assert_eq!(usage.unwrap().prompt_tokens, 100);

        let summary = tracker.summary();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.prompt_tokens, 100);
        assert_eq!(summary.completion_tokens, 50);
        assert_eq!(
            summary.usage_by_provider.get("fixed_usage_mock"),
            Some(&1)
        );
    }

    #[tokio::test]
    async fn test_chat_records_usage_and_passes_response_through() {
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 20,
            completion_tokens: 10,
        });
        let tracker = Arc::new(CostTracker::new());
        let wrapped = CostTrackingProvider::new(inner, tracker.clone());

        let response = wrapped.chat(&[], &[], "phi-3-mini", None).await.unwrap();
        assert_eq!(response.message.content, "response");

        let summary = tracker.summary();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.prompt_tokens, 20);
        assert_eq!(summary.completion_tokens, 10);
    }

    #[tokio::test]
    async fn test_local_model_usage_is_tracked_but_free() {
        // The point of wiring this up: local usage should now show real
        // token counts in CostSummary, while still costing $0 (see the
        // `estimate_cost` fix in pool.rs — checked by provider name, not
        // model name, so this covers any local model).
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 1000,
            completion_tokens: 500,
        });
        // Rename via a thin shim so `.name()` reports "local", matching
        // what `estimate_cost` checks.
        struct AsLocal(Arc<dyn Provider>);
        #[async_trait]
        impl Provider for AsLocal {
            async fn complete(
                &self,
                p: &str,
                m: &str,
                o: Option<&ProviderOptions>,
            ) -> anyhow::Result<(String, Option<TokenUsage>)> {
                self.0.complete(p, m, o).await
            }
            async fn chat(
                &self,
                msgs: &[ChatMessage],
                tools: &[Tool],
                m: &str,
                o: Option<&ProviderOptions>,
            ) -> anyhow::Result<ChatResponse> {
                self.0.chat(msgs, tools, m, o).await
            }
            fn name(&self) -> &str {
                "local"
            }
        }

        let tracker = Arc::new(CostTracker::new());
        let wrapped = CostTrackingProvider::new(Arc::new(AsLocal(inner)), tracker.clone());
        wrapped
            .complete("prompt", "phi-3-mini", None)
            .await
            .unwrap();

        let summary = tracker.summary();
        assert_eq!(summary.total_tokens, 1500, "tokens should still be counted");
        assert_eq!(
            summary.estimated_cost_usd, 0.0,
            "local inference must stay free even though it's now tracked"
        );
    }

    #[tokio::test]
    async fn test_failed_call_records_nothing() {
        let inner = Arc::new(AlwaysErrorsProvider);
        let tracker = Arc::new(CostTracker::new());
        let wrapped = CostTrackingProvider::new(inner, tracker.clone());

        let result = wrapped.complete("prompt", "model", None).await;
        assert!(result.is_err());

        let summary = tracker.summary();
        assert_eq!(summary.total_calls, 0, "a failed call has no usage to record");
    }

    #[tokio::test]
    async fn test_name_delegates_to_inner_provider() {
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 1,
            completion_tokens: 1,
        });
        let tracker = Arc::new(CostTracker::new());
        let wrapped = CostTrackingProvider::new(inner, tracker);
        assert_eq!(wrapped.name(), "fixed_usage_mock");
    }
}
