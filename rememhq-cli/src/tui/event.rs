//! Event definitions for the TUI main loop.

use rememhq_core::memory::types::{
    KnowledgeGraphUpdate, MemoryRecord, MemoryResult, MemoryVersionRecord, SessionSummaryRecord,
};
use rememhq_core::reasoning::ReasoningEvent;
use rememhq_core::storage::StoreStats;
use uuid::Uuid;

/// Result of an asynchronous data fetch task spawned by `data.rs`.
#[derive(Debug)]
pub enum FetchResult {
    /// A memory list query completed.
    Memories(anyhow::Result<Vec<MemoryRecord>>),
    /// A paged memory list query completed (records, total count).
    PagedMemories(anyhow::Result<(Vec<MemoryRecord>, usize)>),
    /// A guided LLM recall query completed.
    Recall(anyhow::Result<Vec<MemoryResult>>),
    /// Store statistics fetch completed.
    Stats(anyhow::Result<StoreStats>),
    /// An archive operation completed for a memory UUID.
    Archived(Uuid, anyhow::Result<bool>),
    /// An unarchive operation completed for a memory UUID.
    Unarchived(Uuid, anyhow::Result<bool>),
    /// A hard delete operation completed for a memory UUID.
    Deleted(Uuid, anyhow::Result<bool>),
    /// A decay operation completed.
    Decayed(anyhow::Result<usize>),
    /// Memory record update completed.
    Updated(Uuid, anyhow::Result<()>),
    /// New memory record insertion completed.
    Created(anyhow::Result<MemoryRecord>),
    /// Memory version history fetch completed.
    Versions(Uuid, anyhow::Result<Vec<MemoryVersionRecord>>),
    /// Knowledge Graph triples query completed.
    Graph(anyhow::Result<Vec<KnowledgeGraphUpdate>>),
    /// Session summaries query completed.
    Sessions(anyhow::Result<Vec<SessionSummaryRecord>>),
    /// Bulk archive operation completed for multiple UUIDs.
    BulkArchived(Vec<Uuid>, anyhow::Result<usize>),
    /// Bulk delete operation completed for multiple UUIDs.
    BulkDeleted(Vec<Uuid>, anyhow::Result<usize>),
    /// Session consolidation task completed for a session ID.
    ConsolidatedSession(
        String,
        anyhow::Result<rememhq_core::memory::types::ConsolidationReport>,
    ),
    /// A live streaming ReasoningEvent read from events.jsonl IPC file.
    LiveEvent(ReasoningEvent),
}

/// All events the main loop can receive.
#[derive(Debug)]
pub enum AppEvent {
    /// Terminal input event.
    Input(crossterm::event::Event),
    /// Periodic tick for UI redraw.
    Tick,
    /// Background data fetch completed.
    FetchComplete(FetchResult),
    /// In-process reasoning event broadcast.
    Reasoning(ReasoningEvent),
}
