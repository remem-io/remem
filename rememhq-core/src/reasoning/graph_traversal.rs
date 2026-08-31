//! Multi-hop Knowledge Graph Traversal and Relational Inference Engine.

use crate::memory::types::KnowledgeGraphUpdate;
use crate::storage::MemoryStore;
use std::collections::{HashSet, VecDeque};

/// Path of knowledge triples connecting a source entity to a target entity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgePath {
    pub start_entity: String,
    pub target_entity: String,
    pub hops: usize,
    pub triples: Vec<KnowledgeGraphUpdate>,
}

/// Traversal engine for multi-hop graph reasoning and neighborhood expansion.
pub struct GraphTraversalEngine<'a> {
    store: &'a dyn MemoryStore,
}

impl<'a> GraphTraversalEngine<'a> {
    pub fn new(store: &'a dyn MemoryStore) -> Self {
        Self { store }
    }

    /// Perform Breadth-First Search (BFS) to find the shortest relational path between two entities.
    pub async fn find_path(
        &self,
        start_entity: &str,
        target_entity: &str,
        max_depth: usize,
    ) -> anyhow::Result<Option<KnowledgePath>> {
        if start_entity.eq_ignore_ascii_case(target_entity) {
            return Ok(Some(KnowledgePath {
                start_entity: start_entity.to_string(),
                target_entity: target_entity.to_string(),
                hops: 0,
                triples: Vec::new(),
            }));
        }

        let mut queue: VecDeque<(String, Vec<KnowledgeGraphUpdate>)> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        queue.push_back((start_entity.to_lowercase(), Vec::new()));
        visited.insert(start_entity.to_lowercase());

        while let Some((current_entity, current_path)) = queue.pop_front() {
            if current_path.len() >= max_depth {
                continue;
            }

            let direct_triples = self.store.get_knowledge_for_entity(&current_entity).await?;

            for triple in direct_triples {
                let neighbor = if triple.subject.eq_ignore_ascii_case(&current_entity) {
                    triple.object.clone()
                } else {
                    triple.subject.clone()
                };

                let mut new_path = current_path.clone();
                new_path.push(triple);

                if neighbor.eq_ignore_ascii_case(target_entity) {
                    return Ok(Some(KnowledgePath {
                        start_entity: start_entity.to_string(),
                        target_entity: target_entity.to_string(),
                        hops: new_path.len(),
                        triples: new_path,
                    }));
                }

                let neighbor_norm = neighbor.to_lowercase();
                if visited.insert(neighbor_norm.clone()) {
                    queue.push_back((neighbor_norm, new_path));
                }
            }
        }

        Ok(None)
    }

    /// Retrieve the N-depth knowledge neighborhood around an entity.
    pub async fn get_neighborhood(
        &self,
        start_entity: &str,
        depth: usize,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        let mut collected_triples: Vec<KnowledgeGraphUpdate> = Vec::new();
        let mut seen_keys: HashSet<(String, String, String)> = HashSet::new();
        let mut current_frontier: HashSet<String> = HashSet::new();
        let mut visited_entities: HashSet<String> = HashSet::new();

        current_frontier.insert(start_entity.to_lowercase());

        for _ in 0..depth.max(1) {
            let mut next_frontier: HashSet<String> = HashSet::new();

            for entity in current_frontier {
                if !visited_entities.insert(entity.clone()) {
                    continue;
                }

                let triples = self.store.get_knowledge_for_entity(&entity).await?;

                for t in triples {
                    let key = (
                        t.subject.to_lowercase(),
                        t.predicate.to_lowercase(),
                        t.object.to_lowercase(),
                    );
                    if seen_keys.insert(key) {
                        next_frontier.insert(t.subject.to_lowercase());
                        next_frontier.insert(t.object.to_lowercase());
                        collected_triples.push(t);
                    }
                }
            }

            current_frontier = next_frontier;
            if current_frontier.is_empty() {
                break;
            }
        }

        Ok(collected_triples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::{KnowledgeGraphUpdate, MemoryRecord, MemoryType};
    use crate::storage::sqlite::SqliteStore;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_bfs_path_traversal() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mem_id = Uuid::new_v4();

        let mut memory =
            MemoryRecord::new("User prefers Rust created by Graydon", MemoryType::Fact);
        memory.id = mem_id;
        store.insert(&memory).await.unwrap();

        // user -> prefers -> rust
        store
            .insert_knowledge_triple(
                &KnowledgeGraphUpdate {
                    subject: "User".to_string(),
                    predicate: "prefers".to_string(),
                    object: "Rust".to_string(),
                },
                mem_id,
            )
            .await
            .unwrap();

        // rust -> created_by -> graydon
        store
            .insert_knowledge_triple(
                &KnowledgeGraphUpdate {
                    subject: "Rust".to_string(),
                    predicate: "created_by".to_string(),
                    object: "Graydon".to_string(),
                },
                mem_id,
            )
            .await
            .unwrap();

        let engine = GraphTraversalEngine::new(&store);
        let path = engine.find_path("User", "Graydon", 3).await.unwrap();

        assert!(path.is_some());
        let p = path.unwrap();
        assert_eq!(p.hops, 2);
        assert_eq!(p.triples.len(), 2);
    }
}
