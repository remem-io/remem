pub mod eval;
pub mod react;

use async_trait::async_trait;

pub enum TerminationCondition {
    MaxIterations(usize),
    SuccessCriteriaMet,
}

/// A trait representing an iterative agent execution loop.
#[async_trait]
pub trait AgentLoop: Send + Sync {
    /// Run the loop to completion, returning the final output string.
    async fn run(&mut self) -> anyhow::Result<String>;
}
