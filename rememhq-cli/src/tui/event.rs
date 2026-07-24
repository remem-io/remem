//! Event handling — merges terminal input, engine events, tick, and fetch results.

use rememhq_core::memory::types::{MemoryRecord, MemoryResult};
use rememhq_core::reasoning::ReasoningEvent;
use rememhq_core::storage::StoreStats;
use uuid::Uuid;

/// Results that can arrive from a background fetch task.
#[derive(Debug)]
pub enum FetchResult {
    /// A list/search_fts fetch completed.
    Memories(anyhow::Result<Vec<MemoryRecord>>),
    /// A guided LLM recall query completed.
    Recall(anyhow::Result<Vec<MemoryResult>>),
    /// A stats() fetch completed.
    Stats(anyhow::Result<StoreStats>),
    /// An archive operation completed for a memory UUID.
    Archived(Uuid, anyhow::Result<bool>),
}

/// All events the main loop can receive.
#[derive(Debug)]
pub enum AppEvent {
    /// A crossterm terminal input event.
    Input(crossterm::event::Event),
    /// A reasoning engine event from the broadcast bus.
    Reasoning(ReasoningEvent),
    /// Periodic tick for stats refresh.
    Tick,
    /// A background data fetch completed.
    FetchComplete(FetchResult),
}
