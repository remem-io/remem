use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::MemoryStore;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_concurrent_sqlite_store_access() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test_concurrent.db");
    let store = Arc::new(SqliteStore::open(&db_path)?);

    let mut handles = Vec::new();

    // Spawn 10 concurrent tasks writing memories
    for i in 0..10 {
        let store_clone = store.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..20 {
                let content = format!("Concurrent memory write {}-{}", i, j);
                let rec = MemoryRecord::new(&content, MemoryType::Fact);
                store_clone.insert(&rec).await.unwrap();
            }
        }));
    }

    // Spawn 5 concurrent tasks reading memories and checking stats
    for _ in 0..5 {
        let store_clone = store.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                let _ = store_clone.list(&[], None, None, 100).await;
                let _ = store_clone.stats().await;
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }));
    }

    for h in handles {
        h.await?;
    }

    let stats = store.stats().await?;
    assert_eq!(
        stats.total_memories, 200,
        "all 200 concurrent records must be persisted safely"
    );

    Ok(())
}
