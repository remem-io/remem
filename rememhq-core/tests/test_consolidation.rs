//! Integration tests for the consolidation flow.

use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::providers::mock::{MockEmbeddings, MockProvider};
use rememhq_core::providers::EmbeddingProvider;
use rememhq_core::reasoning::consolidation::consolidate_session;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};
use rememhq_core::storage::MemoryStore;

#[tokio::test]
async fn test_consolidate_empty_session() {
    let store = SqliteStore::open_in_memory().unwrap();
    let index = HNSWVectorIndex::new(768, 100);
    let provider = MockProvider;
    let embeddings = MockEmbeddings::new(768);

    let report = consolidate_session(
        &provider,
        &embeddings,
        &store,
        &index,
        "session-empty",
        "mock-model",
        None,
    )
    .await
    .unwrap();

    assert_eq!(report.session_id, "session-empty");
    assert_eq!(report.new_facts, 0);
    assert_eq!(report.updated_facts, 0);
    assert!(report.contradictions.is_empty());
    assert!(report.knowledge_graph_updates.is_empty());
}

#[tokio::test]
async fn test_consolidate_normal_session() {
    let store = SqliteStore::open_in_memory().unwrap();
    let index = HNSWVectorIndex::new(768, 100);
    let provider = MockProvider;
    let embeddings = MockEmbeddings::new(768);

    // Insert a raw memory from the session
    let record = MemoryRecord::new("User said they love coding in Rust", MemoryType::Fact)
        .with_session("session-normal");
    let _embedding = embeddings.embed(&record.content, None).await.unwrap();
    store.insert(&record).await.unwrap();

    let report = consolidate_session(
        &provider,
        &embeddings,
        &store,
        &index,
        "session-normal",
        "mock-model",
        None,
    )
    .await
    .unwrap();

    assert_eq!(report.session_id, "session-normal");
    assert_eq!(report.new_facts, 1);
    assert_eq!(report.updated_facts, 0);
    assert!(report.contradictions.is_empty());

    // Check that the new consolidated fact "Alice likes Rust" was stored
    let all_memories = store.list(&[], None, None, 100).await.unwrap();
    let consolidated = all_memories
        .iter()
        .find(|m| m.content == "Alice likes Rust")
        .expect("Should find consolidated memory");
    assert_eq!(
        consolidated.source_session.as_deref(),
        Some("session-normal")
    );
}

#[tokio::test]
async fn test_consolidate_procedure_session() {
    let store = SqliteStore::open_in_memory().unwrap();
    let index = HNSWVectorIndex::new(768, 100);
    let provider = MockProvider;
    let embeddings = MockEmbeddings::new(768);

    // Insert raw memory mentioning "To bake a cake"
    let record = MemoryRecord::new(
        "To bake a cake, first preheat the oven and then mix the batter.",
        MemoryType::Procedure,
    )
    .with_session("session-cake");
    store.insert(&record).await.unwrap();

    let report = consolidate_session(
        &provider,
        &embeddings,
        &store,
        &index,
        "session-cake",
        "mock-model",
        None,
    )
    .await
    .unwrap();

    assert_eq!(report.session_id, "session-cake");
    assert_eq!(report.new_facts, 2); // Oven preheat and batter mixing
    assert_eq!(report.knowledge_graph_updates.len(), 1);

    let all_memories = store.list(&[], None, None, 100).await.unwrap();
    let preheat = all_memories
        .iter()
        .any(|m| m.content == "First, preheat the oven");
    let mix = all_memories
        .iter()
        .any(|m| m.content == "Then, mix the batter");
    assert!(preheat);
    assert!(mix);

    // Verify knowledge graph triple exists in DB
    let triples = store.query_knowledge(None, None, None).await.unwrap();
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0].subject, "First, preheat the oven");
    assert_eq!(triples[0].predicate, "next_step");
    assert_eq!(triples[0].object, "Then, mix the batter");
}

#[tokio::test]
async fn test_consolidate_with_contradiction_autoresolve() {
    let store = SqliteStore::open_in_memory().unwrap();
    let index = HNSWVectorIndex::new(768, 100);
    let provider = MockProvider;
    let embeddings = MockEmbeddings::new(768);

    // 1. Insert existing memory "Alice lives in London"
    let mut existing_record = MemoryRecord::new("Alice lives in London", MemoryType::Fact);
    let embedding = embeddings.embed(&existing_record.content, None).await.unwrap();
    existing_record.embedding = Some(embedding.clone());
    store.insert(&existing_record).await.unwrap();
    index.add(existing_record.id, &embedding).await.unwrap();

    // Verify it is there initially
    assert!(store.get(existing_record.id).await.unwrap().is_some());

    // 2. Insert new raw memory from the session "Alice moved to New York"
    let raw_record = MemoryRecord::new("Alice moved to New York", MemoryType::Fact)
        .with_session("session-contradiction");
    store.insert(&raw_record).await.unwrap();

    // 3. Consolidate session
    let report = consolidate_session(
        &provider,
        &embeddings,
        &store,
        &index,
        "session-contradiction",
        "mock-model",
        None,
    )
    .await
    .unwrap();

    assert_eq!(report.session_id, "session-contradiction");
    assert_eq!(report.new_facts, 1);
    assert_eq!(report.contradictions.len(), 1);

    // Verify that "Alice lives in London" has been archived (get should return None)
    assert!(store.get(existing_record.id).await.unwrap().is_none());

    // Verify that "Alice moved to New York" was saved as a consolidated memory
    let all_memories = store.list(&[], None, None, 100).await.unwrap();
    let consolidated = all_memories
        .iter()
        .find(|m| m.content == "Alice moved to New York")
        .expect("Should find consolidated memory");
    assert_eq!(
        consolidated.source_session.as_deref(),
        Some("session-contradiction")
    );
}
