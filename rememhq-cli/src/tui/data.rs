//! Async data-fetch functions wrapping ReasoningEngine / MemoryStore.
//!
//! All fetches are spawned as separate tokio tasks so they don't block
//! the event loop (see §6 and §8 of the implementation guide).

use std::sync::Arc;

use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::reasoning::ReasoningEngine;
use rememhq_core::storage::sqlite::SqliteStore;
use rememhq_core::storage::MemoryStore;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::event::FetchResult;

/// Spawn a task that fetches the memory list from the store.
#[allow(dead_code)]
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

/// Spawn a task that fetches paged memories and total count.
pub fn spawn_list_paged_fetch(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    memory_type: Option<MemoryType>,
    page: usize,
    page_size: usize,
    show_archived: bool,
) {
    tokio::spawn(async move {
        let offset = page * page_size;
        let result = store
            .list_paged(&[], memory_type, None, page_size, offset, show_archived)
            .await;
        let _ = tx.send(FetchResult::PagedMemories(result));
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

/// Spawn a task that unarchives a memory by ID.
pub fn spawn_unarchive_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    id: Uuid,
) {
    tokio::spawn(async move {
        let result = store.unarchive(id).await;
        let _ = tx.send(FetchResult::Unarchived(id, result));
    });
}

/// Spawn a task that hard deletes a memory by ID.
pub fn spawn_delete_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    id: Uuid,
) {
    tokio::spawn(async move {
        let result = store.delete(id).await;
        let _ = tx.send(FetchResult::Deleted(id, result));
    });
}

/// Spawn a task that applies decay across memories.
pub fn spawn_decay_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    factor: f32,
) {
    tokio::spawn(async move {
        let result = store.apply_decay(factor).await;
        let _ = tx.send(FetchResult::Decayed(result));
    });
}

/// Spawn a task that updates a memory record in the store.
pub fn spawn_update_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    record: MemoryRecord,
) {
    tokio::spawn(async move {
        let id = record.id;
        let result = store.update(&record).await;
        let _ = tx.send(FetchResult::Updated(id, result));
    });
}

/// Spawn a task that creates/inserts a new memory record in the store.
pub fn spawn_create_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    record: MemoryRecord,
) {
    tokio::spawn(async move {
        let rec = record.clone();
        let result = store.insert(&rec).await.map(|_| rec);
        let _ = tx.send(FetchResult::Created(result));
    });
}

/// Spawn a task that fetches version history for a memory record.
pub fn spawn_versions_fetch(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    memory_id: Uuid,
) {
    tokio::spawn(async move {
        let result = store.list_memory_versions("default", memory_id).await;
        let _ = tx.send(FetchResult::Versions(memory_id, result));
    });
}

/// Spawn a task that fetches Knowledge Graph triples.
pub fn spawn_graph_fetch(store: Arc<SqliteStore>, tx: mpsc::UnboundedSender<FetchResult>) {
    tokio::spawn(async move {
        let result = store.query_knowledge(None, None, None).await;
        let _ = tx.send(FetchResult::Graph(result));
    });
}

/// Spawn a task that fetches recent session summaries.
pub fn spawn_sessions_fetch(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    project: String,
) {
    tokio::spawn(async move {
        let result = store.get_recent_session_summaries(&project, 50).await;
        let _ = tx.send(FetchResult::Sessions(result));
    });
}

/// Spawn a task that performs bulk archive across multiple UUIDs.
pub fn spawn_bulk_archive_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    ids: Vec<Uuid>,
) {
    tokio::spawn(async move {
        let mut count = 0;
        for id in &ids {
            if matches!(store.archive(*id).await, Ok(true)) {
                count += 1;
            }
        }
        let _ = tx.send(FetchResult::BulkArchived(ids, Ok(count)));
    });
}

/// Spawn a task that performs bulk hard delete across multiple UUIDs.
pub fn spawn_bulk_delete_task(
    store: Arc<SqliteStore>,
    tx: mpsc::UnboundedSender<FetchResult>,
    ids: Vec<Uuid>,
) {
    tokio::spawn(async move {
        let mut count = 0;
        for id in &ids {
            if matches!(store.delete(*id).await, Ok(true)) {
                count += 1;
            }
        }
        let _ = tx.send(FetchResult::BulkDeleted(ids, Ok(count)));
    });
}

/// Spawn a task that triggers LLM consolidation on a session.
pub fn spawn_consolidate_session_task(
    engine: Arc<ReasoningEngine>,
    tx: mpsc::UnboundedSender<FetchResult>,
    session_id: String,
) {
    tokio::spawn(async move {
        let result = engine.consolidate_session(&session_id).await;
        let _ = tx.send(FetchResult::ConsolidatedSession(session_id, result));
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
