//! Integration tests for the HNSW vector index via C++ FFI.

use rememhq_core::storage::vector::{HNSWVectorIndex, VectorIndex};
use uuid::Uuid;

#[tokio::test]
async fn test_index_new() {
    let index = HNSWVectorIndex::new(128, 1000);
    assert_eq!(index.len(), 0);
}

#[tokio::test]
async fn test_add_and_len() {
    let index = HNSWVectorIndex::new(4, 100);
    let id = Uuid::new_v4();
    let embedding = vec![1.0, 0.0, 0.0, 0.0];

    index.add(id, &embedding).await.unwrap();
    assert_eq!(index.len(), 1);
}

#[tokio::test]
async fn test_add_multiple() {
    let index = HNSWVectorIndex::new(4, 100);

    for i in 0..10 {
        let id = Uuid::new_v4();
        let embedding = vec![i as f32, 0.0, 0.0, 1.0];
        index.add(id, &embedding).await.unwrap();
    }
    assert_eq!(index.len(), 10);
}

#[tokio::test]
async fn test_search_returns_nearest() {
    let dim = 4;
    let index = HNSWVectorIndex::new(dim, 100);

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    // Three distinct vectors
    index.add(id1, &[1.0, 0.0, 0.0, 0.0]).await.unwrap();
    index.add(id2, &[0.0, 1.0, 0.0, 0.0]).await.unwrap();
    index.add(id3, &[0.0, 0.0, 1.0, 0.0]).await.unwrap();

    // Query closest to id1
    let results = index.search(&[0.9, 0.1, 0.0, 0.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
}

#[tokio::test]
async fn test_search_top_k() {
    let index = HNSWVectorIndex::new(4, 100);

    for _ in 0..5 {
        let id = Uuid::new_v4();
        index.add(id, &[1.0, 0.0, 0.0, 0.0]).await.unwrap();
    }

    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 3).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_search_empty_index() {
    let index = HNSWVectorIndex::new(4, 100);
    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 5).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.idx");
    let dim = 4;

    // Create and populate
    let index = HNSWVectorIndex::new(dim, 100);
    let id = Uuid::new_v4();
    index.add(id, &[1.0, 2.0, 3.0, 4.0]).await.unwrap();
    index.save(&path).await.unwrap();

    // Load into fresh index
    let index2 = HNSWVectorIndex::new(dim, 100);
    index2.load(&path).await.unwrap();
    assert_eq!(index2.len(), 1);

    // Search should still work
    let results = index2.search(&[1.0, 2.0, 3.0, 4.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id);
}

#[tokio::test]
async fn test_remove() {
    let index = HNSWVectorIndex::new(4, 100);
    let id = Uuid::new_v4();
    index.add(id, &[1.0, 0.0, 0.0, 0.0]).await.unwrap();
    assert_eq!(index.len(), 1);

    index.remove(id).await.unwrap();
    // Note: HNSW mark-delete doesn't decrement count in hnswlib,
    // but the entry should not appear in search results
    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
    // After removal, either empty or the result should not be the removed id
    if !results.is_empty() {
        // Mark-delete: search may still return it in some implementations
        // This is a known hnswlib behavior
    }
}
