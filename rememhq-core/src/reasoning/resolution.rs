use crate::memory::types::KnowledgeGraphUpdate;
use crate::providers::Provider;
use crate::storage::MemoryStore;
use anyhow::Result;

/// Resolves entities in Knowledge Graph updates to ensure consistency.
#[async_trait::async_trait]
pub trait EntityResolver: Send + Sync {
    /// Resolve entities in a set of KG updates.
    async fn resolve(
        &self,
        updates: Vec<KnowledgeGraphUpdate>,
    ) -> Result<Vec<KnowledgeGraphUpdate>>;
}

pub struct LlmEntityResolver<'a> {
    provider: &'a dyn Provider,
    model: String,
    store: &'a crate::storage::sqlite::SqliteStore,
}

impl<'a> LlmEntityResolver<'a> {
    pub fn new(
        provider: &'a dyn Provider,
        model: String,
        store: &'a crate::storage::sqlite::SqliteStore,
    ) -> Self {
        Self {
            provider,
            model,
            store,
        }
    }

    async fn resolve_entity(&self, entity: &str) -> Result<String> {
        // 1. Query for similar entities (simple exact match check first for speed)
        let existing: Vec<KnowledgeGraphUpdate> =
            self.store.query_knowledge(Some(entity), None, None).await?;
        if !existing.is_empty() {
            return Ok(entity.to_string());
        }

        // 2. Fetch a list of "likely" candidates from the store
        // For now, we'll just use a simple "recent entities" or "similar prefix" approach
        // In a full implementation, this would use vector search on entity names.
        let candidates: Vec<String> = self.store.list_recent_entities(20).await?;

        if candidates.is_empty() {
            return Ok(entity.to_string());
        }

        // 3. Ask LLM to resolve
        let prompt = format!(
            "You are an entity resolution engine. Given a new entity and a list of existing entities, decide if the new entity refers to one of the existing ones.
            
New Entity: \"{}\"

Existing Entities:
{}

If the new entity matches an existing one, return ONLY the existing entity name.
If it is a new entity, return ONLY the new entity name.
Do not provide any explanation.",
            entity,
            candidates.join("\n")
        );

        let resolved = self.provider.complete(&prompt, &self.model).await?;
        let resolved = resolved.trim().trim_matches('"').to_string();

        if resolved != entity {
            tracing::info!(original = %entity, resolved = %resolved, "Resolved entity");
        }

        Ok(resolved)
    }
}

#[async_trait::async_trait]
impl<'a> EntityResolver for LlmEntityResolver<'a> {
    async fn resolve(
        &self,
        updates: Vec<KnowledgeGraphUpdate>,
    ) -> Result<Vec<KnowledgeGraphUpdate>> {
        let mut resolved_updates = Vec::new();

        for mut update in updates {
            update.subject = self.resolve_entity(&update.subject).await?;
            update.object = self.resolve_entity(&update.object).await?;
            resolved_updates.push(update);
        }

        Ok(resolved_updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::{KnowledgeGraphUpdate, MemoryRecord, MemoryType};
    use crate::providers::mock::MockProvider;
    use crate::storage::sqlite::SqliteStore;

    #[tokio::test]
    async fn test_entity_resolution() {
        let store = SqliteStore::open_in_memory().unwrap();
        let provider = MockProvider;

        // Seed some existing knowledge to populate "recent entities"
        // First insert a memory record because of the foreign key constraint
        let record = MemoryRecord::new("PostgreSQL is a database", MemoryType::Fact);
        let memory_id = record.id;
        store.insert(&record).await.unwrap();

        store
            .insert_knowledge_triple(
                &KnowledgeGraphUpdate {
                    subject: "PostgreSQL".to_string(),
                    predicate: "is_a".to_string(),
                    object: "Database".to_string(),
                },
                memory_id,
            )
            .await
            .unwrap();

        let resolver = LlmEntityResolver::new(&provider, "mock".to_string(), &store);

        // MockProvider should return "PostgreSQL" if prompt contains "Postgres"
        // (Wait, I need to update MockProvider to handle this specific test case)

        let updates = vec![KnowledgeGraphUpdate {
            subject: "Postgres".to_string(),
            predicate: "uses".to_string(),
            object: "Port 5432".to_string(),
        }];

        let resolved = resolver.resolve(updates).await.unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].subject, "PostgreSQL"); // Resolved from "Postgres"
        assert_eq!(resolved[0].object, "Port 5432"); // Stayed same
    }
}
