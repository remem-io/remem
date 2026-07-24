//! Async data-fetch functions wrapping ReasoningEngine / MemoryStore.
//!
//! All fetches are spawned as separate tokio tasks so they don't block
//! the event loop (see §6 and §8 of the implementation guide).

use std::sync::Arc;

use rememhq_core::memory::types::MemoryType;
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::MemoryStore;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::event::FetchResult;

/// Spawn a task that fetches the memory list from the store.
pub fn spawn_list_fetch(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    memory_type: Option<MemoryType>,
    limit: usize,
) {
    tokio::spawn(async move {
        let result = store.list(&[], memory_type, None, limit).await;
        let _ = tx.send(FetchResult::Memories(result));
    });
}

/// Spawn a task that performs a full-text search.
pub fn spawn_search_fetch(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    query: String,
    limit: usize,
) {
    tokio::spawn(async move {
        let result = store.search_fts(&query, limit).await;
        let _ = tx.send(FetchResult::Memories(result));
    });
}

/// Spawn a task that performs guided LLM recall.
pub fn spawn_recall_fetch(
    engine: Arc<ReasoningEngine>,
    tx: mpsc::UnboundedSender<FetchResult>,
    query: String,
    limit: usize,
) {
    tokio::spawn(async move {
        let result = engine.recall(&query, limit, &[], None, None, None).await;
        let _ = tx.send(FetchResult::Recall(result));
    });
}

/// Spawn a task that fetches store statistics.
pub fn spawn_stats_fetch(store: Arc<SqliteStore>, tx: mpsc::UnboundedSender<FetchResult>) {
    tokio::spawn(async move {
        let result = store.stats().await;
        let _ = tx.send(FetchResult::Stats(result));
    });
}

/// Spawn a task that archives a memory by ID.
pub fn spawn_archive_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    id: Uuid,
) {
    tokio::spawn(async move {
        let result = store.archive(id).await;
        let _ = tx.send(FetchResult::Archived(id, result));
    });
}
