//! Cost-tracking + audit-logging `Provider` decorator.
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
//!
//! It also writes a persistent [`InferenceLogEntry`] per call when an
//! audit store is configured — unlike `CostTracker` (in-memory running
//! totals only), this survives a restart and records latency and
//! failures too, not just successful token counts. See
//! `storage::inference_log` for why that's a separate thing from both
//! `CostTracker` and `storage::audit::AuditEntry`.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use super::pool::CostTracker;
use super::{ChatMessage, ChatResponse, Provider, ProviderOptions, Tool, TokenUsage};
use crate::storage::sqlite::SqliteStore;
use crate::storage::InferenceLogEntry;

/// Wraps an inner [`Provider`], recording token usage/cost into a shared
/// [`CostTracker`] after every successful call, and — when an audit
/// store is configured — a persistent [`InferenceLogEntry`] after
/// *every* call, success or failure, with latency. Otherwise fully
/// transparent: responses (and errors) pass through unchanged, and
/// [`Provider::name`] delegates to the inner provider so callers/logs
/// still see e.g. `"anthropic"` or `"local"`, not `"cost_tracking"`.
pub struct CostTrackingProvider {
    inner: Arc<dyn Provider>,
    cost_tracker: Arc<CostTracker>,
    /// `None` disables persistent inference logging; in-memory cost
    /// tracking above still applies either way. `None` in tests that
    /// don't need a database, and available to any future caller that
    /// wants cost tracking without the audit trail.
    audit_store: Option<Arc<SqliteStore>>,
}

impl CostTrackingProvider {
    pub fn new(
        inner: Arc<dyn Provider>,
        cost_tracker: Arc<CostTracker>,
        audit_store: Option<Arc<SqliteStore>>,
    ) -> Self {
        Self {
            inner,
            cost_tracker,
            audit_store,
        }
    }

    /// Record `usage` in the in-memory `CostTracker` if present. A call
    /// that errored (and so never produced a response, let alone usage)
    /// or that succeeded without a `usage` block (some local runtimes
    /// omit it — see `providers/local.rs`) records nothing here; there's
    /// nothing accurate to record either way. The persistent audit log
    /// (`log_call`, below) still gets an entry regardless — that's the
    /// point of having both: `CostTracker` is "how much have we spent",
    /// the audit log is "what happened, call by call, including
    /// failures".
    fn record_cost(&self, model: &str, usage: &Option<TokenUsage>) {
        if let Some(u) = usage {
            self.cost_tracker.record_usage(
                self.inner.name(),
                model,
                u.prompt_tokens,
                u.completion_tokens,
            );
        }
    }

    /// Write a persistent `InferenceLogEntry` if an audit store is
    /// configured. Best-effort: a logging failure is warned about, not
    /// propagated — an inference call that otherwise succeeded (or
    /// failed for its own reasons) must not additionally fail just
    /// because its audit entry couldn't be written.
    async fn log_call(
        &self,
        model: &str,
        prompt_hash: String,
        usage: &Option<TokenUsage>,
        latency_ms: u64,
        error: Option<String>,
    ) {
        let Some(store) = &self.audit_store else {
            return;
        };
        let entry = InferenceLogEntry::new(
            self.inner.name(),
            model,
            prompt_hash,
            usage.as_ref().map(|u| u.prompt_tokens),
            usage.as_ref().map(|u| u.completion_tokens),
            latency_ms,
            error,
        );
        if let Err(e) = store.insert_inference_log(&entry).await {
            tracing::warn!(error = %e, "failed to write inference log entry");
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
        let start = Instant::now();
        let result = self.inner.complete(prompt, model, options).await;
        let latency_ms = start.elapsed().as_millis() as u64;
        let prompt_hash = InferenceLogEntry::hash_prompt(prompt);

        match &result {
            Ok((_, usage)) => {
                self.record_cost(model, usage);
                self.log_call(model, prompt_hash, usage, latency_ms, None)
                    .await;
            }
            Err(e) => {
                self.log_call(model, prompt_hash, &None, latency_ms, Some(e.to_string()))
                    .await;
            }
        }

        result
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[Tool],
        model: &str,
        options: Option<&ProviderOptions>,
    ) -> anyhow::Result<ChatResponse> {
        let start = Instant::now();
        let result = self.inner.chat(messages, tools, model, options).await;
        let latency_ms = start.elapsed().as_millis() as u64;
        // chat() has no single "prompt" string — hash a JSON
        // serialization of the message list instead. Good enough for
        // this hash's actual purpose (correlating identical calls
        // without storing their content); doesn't need to be a
        // cryptographically canonical encoding of the conversation.
        let prompt_hash =
            InferenceLogEntry::hash_prompt(&serde_json::to_string(messages).unwrap_or_default());

        match &result {
            Ok(response) => {
                self.record_cost(model, &response.usage);
                self.log_call(model, prompt_hash, &response.usage, latency_ms, None)
                    .await;
            }
            Err(e) => {
                self.log_call(model, prompt_hash, &None, latency_ms, Some(e.to_string()))
                    .await;
            }
        }

        result
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
        let wrapped = CostTrackingProvider::new(inner, tracker.clone(), None);

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
        let wrapped = CostTrackingProvider::new(inner, tracker.clone(), None);

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
        let wrapped = CostTrackingProvider::new(Arc::new(AsLocal(inner)), tracker.clone(), None);
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
        let wrapped = CostTrackingProvider::new(inner, tracker.clone(), None);

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
        let wrapped = CostTrackingProvider::new(inner, tracker, None);
        assert_eq!(wrapped.name(), "fixed_usage_mock");
    }

    #[tokio::test]
    async fn test_complete_writes_inference_log_on_success() {
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 42,
            completion_tokens: 8,
        });
        let tracker = Arc::new(CostTracker::new());
        let store = Arc::new(crate::storage::sqlite::SqliteStore::open_in_memory().unwrap());
        let wrapped = CostTrackingProvider::new(inner, tracker, Some(store.clone()));

        wrapped
            .complete("what is the capital of France?", "phi-3-mini", None)
            .await
            .unwrap();

        let logs = store.get_inference_logs(None, 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].provider, "fixed_usage_mock");
        assert_eq!(logs[0].model, "phi-3-mini");
        assert_eq!(logs[0].prompt_tokens, Some(42));
        assert_eq!(logs[0].completion_tokens, Some(8));
        assert!(logs[0].error.is_none());
        // Hash, not the raw prompt — this is the whole point of hashing it.
        assert_ne!(logs[0].prompt_hash, "what is the capital of France?");
        assert_eq!(
            logs[0].prompt_hash,
            InferenceLogEntry::hash_prompt("what is the capital of France?")
        );
    }

    #[tokio::test]
    async fn test_complete_writes_inference_log_on_failure() {
        // The point of logging on the error path too, unlike CostTracker:
        // a failed call still gets an audit entry, with its error message
        // and no usage — CostTracker (checked separately) correctly has
        // nothing to show for it, but the audit log does.
        let inner = Arc::new(AlwaysErrorsProvider);
        let tracker = Arc::new(CostTracker::new());
        let store = Arc::new(crate::storage::sqlite::SqliteStore::open_in_memory().unwrap());
        let wrapped = CostTrackingProvider::new(inner, tracker, Some(store.clone()));

        let result = wrapped.complete("prompt", "some-model", None).await;
        assert!(result.is_err());

        let logs = store.get_inference_logs(None, 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].error.as_deref(), Some("simulated provider failure"));
        assert!(logs[0].prompt_tokens.is_none());
        assert!(logs[0].completion_tokens.is_none());
    }

    #[tokio::test]
    async fn test_chat_writes_inference_log_with_message_hash() {
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 5,
            completion_tokens: 3,
        });
        let tracker = Arc::new(CostTracker::new());
        let store = Arc::new(crate::storage::sqlite::SqliteStore::open_in_memory().unwrap());
        let wrapped = CostTrackingProvider::new(inner, tracker, Some(store.clone()));

        let messages = [ChatMessage {
            role: ChatRole::User,
            content: "hi there".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];
        wrapped.chat(&messages, &[], "phi-3-mini", None).await.unwrap();

        let logs = store.get_inference_logs(None, 10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].prompt_tokens, Some(5));
        // Same messages should hash the same way if logged again.
        let expected_hash =
            InferenceLogEntry::hash_prompt(&serde_json::to_string(&messages).unwrap());
        assert_eq!(logs[0].prompt_hash, expected_hash);
    }

    #[tokio::test]
    async fn test_no_audit_store_means_no_logging_but_cost_tracking_still_works() {
        // The `None` used by every test above this point isn't just a
        // placeholder to satisfy the constructor — confirm it actually
        // means "no persistent logging" (cost tracking is asserted
        // separately by the tests further up already).
        let inner = Arc::new(FixedUsageProvider {
            prompt_tokens: 1,
            completion_tokens: 1,
        });
        let tracker = Arc::new(CostTracker::new());
        let wrapped = CostTrackingProvider::new(inner, tracker, None);

        wrapped.complete("prompt", "model", None).await.unwrap();
        // Nothing to assert against directly since there's no store — the
        // real assertion is simply that this doesn't panic and the call
        // above completes normally with no audit_store configured.
    }
}
