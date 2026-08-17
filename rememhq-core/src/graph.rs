use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;

/// State trait represents the shared data object that flows along the edges.
pub trait State: Send + Sync + Debug + Clone {}

/// The result of a node's execution, indicating where to go next.
pub enum Edge<S: State> {
    /// Proceed unconditionally or conditionally to the next node by name.
    Next(String, S),
    /// Terminate the graph with the final state.
    End(S),
    /// Terminate the graph with an error.
    Error(anyhow::Error),
}

/// A node represents one specialized agent with one job.
#[async_trait]
pub trait Node<S: State>: Send + Sync {
    /// The name of this node, used for routing.
    fn name(&self) -> &str;

    /// Execute the node's work and return the next edge.
    async fn run(&self, state: S) -> Edge<S>;
}

/// A graph orchestrates a set of nodes and routes state between them.
pub struct Graph<S: State> {
    nodes: HashMap<String, Box<dyn Node<S>>>,
    entry_point: String,
}

impl<S: State> Graph<S> {
    pub fn new(entry_point: impl Into<String>) -> Self {
        Self {
            nodes: HashMap::new(),
            entry_point: entry_point.into(),
        }
    }

    /// Add a specialized node to the graph.
    pub fn add_node(mut self, node: Box<dyn Node<S>>) -> Self {
        self.nodes.insert(node.name().to_string(), node);
        self
    }

    /// Run the graph from the entry point until termination.
    pub async fn run(&self, initial_state: S) -> anyhow::Result<S> {
        let mut current_node_name = self.entry_point.clone();
        let mut current_state = initial_state;

        loop {
            let node = self.nodes.get(&current_node_name).ok_or_else(|| {
                anyhow::anyhow!("Node '{}' not found in graph", current_node_name)
            })?;

            match node.run(current_state).await {
                Edge::Next(next_node, next_state) => {
                    current_node_name = next_node;
                    current_state = next_state;
                }
                Edge::End(final_state) => {
                    return Ok(final_state);
                }
                Edge::Error(err) => {
                    return Err(err);
                }
            }
        }
    }
}
