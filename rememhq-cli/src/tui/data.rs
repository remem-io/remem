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

/// Spawn a background task that tails `events.jsonl` for inter-process live streaming.
pub fn spawn_event_tailer(events_file: std::path::PathBuf, tx: mpsc::UnboundedSender<FetchResult>) {
    tokio::spawn(async move {
        let mut position = 0u64;
        loop {
            if let Ok(mut file) = tokio::fs::File::open(&events_file).await {
                use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
                if let Ok(meta) = file.metadata().await {
                    if meta.len() >= position {
                        let _ = file.seek(std::io::SeekFrom::Start(position)).await;
                        let mut reader = BufReader::new(file);
                        let mut line = String::new();
                        while let Ok(n) = reader.read_line(&mut line).await {
                            if n == 0 {
                                break;
                            }
                            position += n as u64;
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                if let Ok(event) = serde_json::from_str::<
                                    rememhq_core::reasoning::ReasoningEvent,
                                >(trimmed)
                                {
                                    let _ = tx.send(FetchResult::LiveEvent(event));
                                }
                            }
                            line.clear();
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });
}
