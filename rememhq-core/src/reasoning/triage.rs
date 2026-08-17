use crate::graph::{Edge, Graph, Node, State};
use crate::harness::{AgentHarness, Permissions};
use crate::providers::{ChatMessage, ChatRole, Provider};
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TriageState {
    pub ci_logs: String,
    pub failure_type: Option<String>,
    pub fix_attempt: Option<String>,
    pub is_resolved: bool,
    pub escalation_reason: Option<String>,
}

impl State for TriageState {}

/// Node 1: Classify the CI failure
pub struct ClassifierNode {
    pub provider: Arc<dyn Provider>,
    pub model: String,
}

#[async_trait]
impl Node<TriageState> for ClassifierNode {
    fn name(&self) -> &str {
        "classifier"
    }

    async fn run(&self, mut state: TriageState) -> Edge<TriageState> {
        let harness = AgentHarness::new(self.provider.clone());
        let messages = vec![ChatMessage {
            role: ChatRole::User,
            content: format!(
                "Analyze these CI logs and classify the failure type as either 'compile_error', 'test_failure', or 'infrastructure'. Output ONLY the type.\nLogs:\n{}",
                state.ci_logs
            ),
            tool_calls: None,
            tool_call_id: None,
        }];

        match harness.chat_with_retry(&messages, &self.model, None).await {
            Ok(resp) => {
                let classification = resp.message.content.trim().to_lowercase();
                state.failure_type = Some(classification.clone());

                if classification.contains("infrastructure") {
                    Edge::Next("escalator".to_string(), state)
                } else {
                    Edge::Next("fixer".to_string(), state)
                }
            }
            Err(e) => {
                state.escalation_reason = Some(format!("Classifier failed: {}", e));
                Edge::Next("escalator".to_string(), state)
            }
        }
    }
}

/// Node 2: Attempt to generate a fix
pub struct FixerNode {
    pub provider: Arc<dyn Provider>,
    pub model: String,
}

#[async_trait]
impl Node<TriageState> for FixerNode {
    fn name(&self) -> &str {
        "fixer"
    }

    async fn run(&self, mut state: TriageState) -> Edge<TriageState> {
        let harness = AgentHarness::new(self.provider.clone()).with_permissions(Permissions {
            allowed_tools: vec![],
            allow_file_read: true,
            allow_file_write: true,
        });

        let messages = vec![ChatMessage {
            role: ChatRole::User,
            content: format!(
                "Generate a fix for this {} failure based on these logs:\n{}",
                state.failure_type.as_deref().unwrap_or("unknown"),
                state.ci_logs
            ),
            tool_calls: None,
            tool_call_id: None,
        }];

        match harness.chat_with_retry(&messages, &self.model, None).await {
            Ok(resp) => {
                state.fix_attempt = Some(resp.message.content);
                // In a real system, this would apply the fix and run tests.
                // For demonstration, we assume the fix generation succeeded.
                state.is_resolved = true;
                Edge::End(state)
            }
            Err(e) => {
                state.escalation_reason = Some(format!("Fixer failed to generate fix: {}", e));
                Edge::Next("escalator".to_string(), state)
            }
        }
    }
}

/// Node 3: Escalate to a human
pub struct EscalatorNode;

#[async_trait]
impl Node<TriageState> for EscalatorNode {
    fn name(&self) -> &str {
        "escalator"
    }

    async fn run(&self, mut state: TriageState) -> Edge<TriageState> {
        state.is_resolved = false;
        // In a real system, this would page a human or open a Jira ticket.
        Edge::End(state)
    }
}

/// Build and execute the CI Triage graph
pub async fn run_triage_graph(
    provider: Arc<dyn Provider>,
    model: &str,
    ci_logs: String,
) -> anyhow::Result<TriageState> {
    let graph = Graph::new("classifier")
        .add_node(Box::new(ClassifierNode {
            provider: provider.clone(),
            model: model.to_string(),
        }))
        .add_node(Box::new(FixerNode {
            provider: provider.clone(),
            model: model.to_string(),
        }))
        .add_node(Box::new(EscalatorNode));

    let initial_state = TriageState {
        ci_logs,
        failure_type: None,
        fix_attempt: None,
        is_resolved: false,
        escalation_reason: None,
    };

    graph.run(initial_state).await
}
