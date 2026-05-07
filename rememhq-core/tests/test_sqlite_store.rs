//! Integration tests for the SqliteStore backend.

use rememhq_core::memory::types::{KnowledgeGraphUpdate, MemoryRecord, MemoryType};
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::MemoryStore;

fn make_store() -> SqliteStore {
    SqliteStore::open_in_memory().unwrap()
}

fn make_record(content: &str) -> MemoryRecord {
    MemoryRecord::new(content, MemoryType::Fact)
        .with_importance(5.0)
        .with_tags(vec!["test".to_string()])
}

#[tokio::test]
async fn test_insert_and_get() {
    let store = make_store();
    let record = make_record("Alice prefers dark mode");
    let id = record.id;

    store.insert(&record).await.unwrap();
    let retrieved = store.get(id).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "Alice prefers dark mode");
}

#[tokio::test]
async fn test_insert_duplicate_id() {
    let store = make_store();
    let record = make_record("first");
    store.insert(&record).await.unwrap();

    // Inserting same ID again should either update or error — not crash
    let result = store.insert(&record).await;
    let _ = result;
}

#[tokio::test]
async fn test_search_fts_by_content() {
    let store = make_store();

    let r1 = make_record("Rust is a systems programming language");
    let r2 = make_record("Python is great for data science");
    store.insert(&r1).await.unwrap();
    store.insert(&r2).await.unwrap();

    let results = store.search_fts("Rust", 10).await.unwrap();
    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.content.contains("Rust")));
}

#[tokio::test]
async fn test_delete() {
    let store = make_store();
    let record = make_record("ephemeral");
    let id = record.id;

    store.insert(&record).await.unwrap();
    store.delete(id).await.unwrap();

    let result = store.get(id).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_update_record() {
    let store = make_store();
    let mut record = make_record("original content");
    let id = record.id;
    store.insert(&record).await.unwrap();

    record.content = "updated content".to_string();
    store.update(&record).await.unwrap();

    let updated = store.get(id).await.unwrap().unwrap();
    assert_eq!(updated.content, "updated content");
}

#[tokio::test]
async fn test_update_importance() {
    let store = make_store();
    let mut record = make_record("test importance");
    let id = record.id;
    store.insert(&record).await.unwrap();

    record.importance = 9.0;
    store.update(&record).await.unwrap();

    let updated = store.get(id).await.unwrap().unwrap();
    assert!((updated.importance - 9.0).abs() < 0.01);
}

#[tokio::test]
async fn test_archive() {
    let store = make_store();
    let record = make_record("archivable");
    let id = record.id;
    store.insert(&record).await.unwrap();

    store.archive(id).await.unwrap();

    // Archived records should not appear in search
    let results = store.search_fts("archivable", 10).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_list_with_filters() {
    let store = make_store();

    let r1 = make_record("fact one").with_session("session-001");
    let r2 = make_record("fact two").with_session("session-001");
    let r3 = make_record("fact three").with_session("session-002");
    store.insert(&r1).await.unwrap();
    store.insert(&r2).await.unwrap();
    store.insert(&r3).await.unwrap();

    let results = store.list(&[], None, None, 100).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_knowledge_graph_insert() {
    let store = make_store();
    let record = make_record("test entity");
    let memory_id = record.id;
    store.insert(&record).await.unwrap();

    let triple = KnowledgeGraphUpdate {
        subject: "Alice".to_string(),
        predicate: "lives_in".to_string(),
        object: "Berlin".to_string(),
    };
    store
        .insert_knowledge_triple(&triple, memory_id)
        .await
        .unwrap();

    // Inserting same triple again should not fail (INSERT OR IGNORE)
    let result = store.insert_knowledge_triple(&triple, memory_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_stats() {
    let store = make_store();
    store.insert(&make_record("stat1")).await.unwrap();
    store.insert(&make_record("stat2")).await.unwrap();

    let stats = store.stats().await.unwrap();
    assert_eq!(stats.total_memories, 2);
}

#[tokio::test]
async fn test_apply_decay() {
    let store = make_store();
    store.insert(&make_record("decaying memory")).await.unwrap();

    let affected = store.apply_decay(0.95).await.unwrap();
    assert!(affected >= 1);
}
