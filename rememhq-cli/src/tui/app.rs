//! Application state and update logic for the TUI.

use std::collections::VecDeque;

use rememhq_core::memory::types::{MemoryRecord, MemoryResult, MemoryType};
use rememhq_core::reasoning::ReasoningEvent;
use rememhq_core::storage::StoreStats;
use uuid::Uuid;

/// Which pane / input mode / modal overlay the TUI is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Default: memory list focused.
    Browse,
    /// Filter/search input has focus.
    Search,
    /// Guided recall query input (`:recall`).
    RecallQuery,
    /// Detail pane for the selected memory is focused.
    Detail,
    /// Stats dashboard pane is focused.
    Stats,
    /// Consolidation monitor pane active.
    Monitor,
    /// Confirmation dialog before archiving a memory.
    ConfirmArchive,
    /// Help cheat-sheet overlay.
    Help,
}

/// Sort field for the browser table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Importance,
    Decay,
    CreatedAt,
}

impl SortField {
    /// Cycle to the next sort field.
    pub fn next(self) -> Self {
        match self {
            SortField::Importance => SortField::Decay,
            SortField::Decay => SortField::CreatedAt,
            SortField::CreatedAt => SortField::Importance,
        }
    }
}

impl std::fmt::Display for SortField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortField::Importance => write!(f, "importance"),
            SortField::Decay => write!(f, "decay"),
            SortField::CreatedAt => write!(f, "created"),
        }
    }
}

/// Memory type filter option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFilter {
    All,
    Fact,
    Procedure,
    Preference,
    Decision,
    Observation,
}

impl TypeFilter {
    /// Cycle to the next memory type filter.
    pub fn next(self) -> Self {
        match self {
            TypeFilter::All => TypeFilter::Fact,
            TypeFilter::Fact => TypeFilter::Procedure,
            TypeFilter::Procedure => TypeFilter::Preference,
            TypeFilter::Preference => TypeFilter::Decision,
            TypeFilter::Decision => TypeFilter::Observation,
            TypeFilter::Observation => TypeFilter::All,
        }
    }

    /// Convert to Option<MemoryType> for store queries.
    pub fn to_memory_type(self) -> Option<MemoryType> {
        match self {
            TypeFilter::All => None,
            TypeFilter::Fact => Some(MemoryType::Fact),
            TypeFilter::Procedure => Some(MemoryType::Procedure),
            TypeFilter::Preference => Some(MemoryType::Preference),
            TypeFilter::Decision => Some(MemoryType::Decision),
            TypeFilter::Observation => Some(MemoryType::Observation),
        }
    }
}

impl std::fmt::Display for TypeFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeFilter::All => write!(f, "all"),
            TypeFilter::Fact => write!(f, "fact"),
            TypeFilter::Procedure => write!(f, "procedure"),
            TypeFilter::Preference => write!(f, "pref"),
            TypeFilter::Decision => write!(f, "decision"),
            TypeFilter::Observation => write!(f, "obs"),
        }
    }
}

/// Top-level application state.
pub struct App {
    /// Current UI mode.
    pub mode: Mode,
    /// Mode to restore when returning from a modal.
    pub previous_mode: Mode,
    /// Fetched memory records for the browser.
    pub memories: Vec<MemoryRecord>,
    /// Results from guided LLM recall query.
    pub recall_results: Vec<MemoryResult>,
    /// Index of the selected row in the browser.
    pub selected: usize,
    /// Current filter/search input text.
    pub filter_input: String,
    /// Cursor position within `filter_input`.
    pub filter_cursor: usize,
    /// Guided recall query input text.
    pub recall_input: String,
    /// Cursor position within `recall_input`.
    pub recall_cursor: usize,
    /// Consolidation event log ring buffer.
    pub consolidation_log: VecDeque<ReasoningEvent>,
    /// Latest store statistics.
    pub stats: Option<StoreStats>,
    /// Memory ID targeted for archiving.
    pub archive_target: Option<Uuid>,
    /// Status message bar (e.g. "Archived memory 12345678").
    pub status_message: Option<(String, std::time::Instant)>,
    /// Whether a data fetch is in progress.
    pub loading: bool,
    /// Signal to exit the main loop.
    pub should_quit: bool,
    /// Current sort field for the browser.
    pub sort_field: SortField,
    /// Sort ascending (true) or descending (false).
    pub sort_ascending: bool,
    /// Active MemoryType filter.
    pub type_filter: TypeFilter,
    /// Scroll offset for the detail pane.
    pub detail_scroll: u16,
}

impl App {
    /// Create a new App with sensible defaults.
    pub fn new() -> Self {
        Self {
            mode: Mode::Browse,
            previous_mode: Mode::Browse,
            memories: Vec::new(),
            recall_results: Vec::new(),
            selected: 0,
            filter_input: String::new(),
            filter_cursor: 0,
            recall_input: String::new(),
            recall_cursor: 0,
            consolidation_log: VecDeque::with_capacity(200),
            stats: None,
            archive_target: None,
            status_message: None,
            loading: true,
            should_quit: false,
            sort_field: SortField::CreatedAt,
            sort_ascending: false,
            type_filter: TypeFilter::All,
            detail_scroll: 0,
        }
    }

    /// Get the currently selected memory, if any.
    pub fn selected_memory(&self) -> Option<&MemoryRecord> {
        self.memories.get(self.selected)
    }

    /// Set a status bar message visible for 4 seconds.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), std::time::Instant::now()));
    }

    /// Get current active status message if not expired.
    pub fn active_status(&self) -> Option<&str> {
        if let Some((ref msg, instant)) = self.status_message {
            if instant.elapsed() < std::time::Duration::from_secs(4) {
                return Some(msg.as_str());
            }
        }
        None
    }

    /// Move selection up in the browser.
    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.detail_scroll = 0;
    }

    /// Move selection down in the browser.
    pub fn select_next(&mut self) {
        if !self.memories.is_empty() && self.selected < self.memories.len() - 1 {
            self.selected += 1;
        }
        self.detail_scroll = 0;
    }

    /// Page up in browser (10 items).
    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
        self.detail_scroll = 0;
    }

    /// Page down in browser (10 items).
    pub fn page_down(&mut self) {
        if !self.memories.is_empty() {
            self.selected = (self.selected + 10).min(self.memories.len() - 1);
        }
        self.detail_scroll = 0;
    }

    /// Select first item in browser.
    pub fn select_first(&mut self) {
        self.selected = 0;
        self.detail_scroll = 0;
    }

    /// Select last item in browser.
    pub fn select_last(&mut self) {
        if !self.memories.is_empty() {
            self.selected = self.memories.len() - 1;
        }
        self.detail_scroll = 0;
    }

    /// Sort the current memory list by the active sort field.
    pub fn sort_memories(&mut self) {
        match self.sort_field {
            SortField::Importance => {
                self.memories.sort_by(|a, b| {
                    a.importance
                        .partial_cmp(&b.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortField::Decay => {
                self.memories.sort_by(|a, b| {
                    a.decay_score
                        .partial_cmp(&b.decay_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortField::CreatedAt => {
                self.memories.sort_by_key(|a| a.created_at);
            }
        }
        if !self.sort_ascending {
            self.memories.reverse();
        }
        // Clamp selection after sort.
        if self.selected >= self.memories.len() && !self.memories.is_empty() {
            self.selected = self.memories.len() - 1;
        }
    }

    /// Push a consolidation event into the ring buffer.
    pub fn push_event(&mut self, event: ReasoningEvent) {
        if self.consolidation_log.len() >= 200 {
            self.consolidation_log.pop_front();
        }
        self.consolidation_log.push_back(event);
    }

    // --- Search Filter Input Helpers ---

    pub fn filter_insert_char(&mut self, c: char) {
        self.filter_input.insert(self.filter_cursor, c);
        self.filter_cursor += c.len_utf8();
    }

    pub fn filter_backspace(&mut self) {
        if self.filter_cursor > 0 {
            let prev = self.filter_input[..self.filter_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.filter_input.drain(prev..self.filter_cursor);
            self.filter_cursor = prev;
        }
    }

    pub fn filter_cursor_left(&mut self) {
        if self.filter_cursor > 0 {
            self.filter_cursor = self.filter_input[..self.filter_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn filter_cursor_right(&mut self) {
        if self.filter_cursor < self.filter_input.len() {
            self.filter_cursor = self.filter_input[self.filter_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.filter_cursor + i)
                .unwrap_or(self.filter_input.len());
        }
    }

    // --- Recall Query Input Helpers ---

    pub fn recall_insert_char(&mut self, c: char) {
        self.recall_input.insert(self.recall_cursor, c);
        self.recall_cursor += c.len_utf8();
    }

    pub fn recall_backspace(&mut self) {
        if self.recall_cursor > 0 {
            let prev = self.recall_input[..self.recall_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.recall_input.drain(prev..self.recall_cursor);
            self.recall_cursor = prev;
        }
    }

    pub fn recall_cursor_left(&mut self) {
        if self.recall_cursor > 0 {
            self.recall_cursor = self.recall_input[..self.recall_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn recall_cursor_right(&mut self) {
        if self.recall_cursor < self.recall_input.len() {
            self.recall_cursor = self.recall_input[self.recall_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.recall_cursor + i)
                .unwrap_or(self.recall_input.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_field_cycles() {
        assert_eq!(SortField::Importance.next(), SortField::Decay);
        assert_eq!(SortField::Decay.next(), SortField::CreatedAt);
        assert_eq!(SortField::CreatedAt.next(), SortField::Importance);
    }

    #[test]
    fn type_filter_cycles() {
        assert_eq!(TypeFilter::All.next(), TypeFilter::Fact);
        assert_eq!(TypeFilter::Fact.next(), TypeFilter::Procedure);
        assert_eq!(TypeFilter::Procedure.next(), TypeFilter::Preference);
        assert_eq!(TypeFilter::Preference.next(), TypeFilter::Decision);
        assert_eq!(TypeFilter::Decision.next(), TypeFilter::Observation);
        assert_eq!(TypeFilter::Observation.next(), TypeFilter::All);
    }

    #[test]
    fn selection_clamps_to_bounds() {
        let mut app = App::new();
        app.select_previous();
        app.select_next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn filter_input_operations() {
        let mut app = App::new();
        app.filter_insert_char('h');
        app.filter_insert_char('i');
        assert_eq!(app.filter_input, "hi");
        assert_eq!(app.filter_cursor, 2);
        app.filter_backspace();
        assert_eq!(app.filter_input, "h");
        assert_eq!(app.filter_cursor, 1);
    }

    #[test]
    fn push_event_caps_at_200() {
        let mut app = App::new();
        for i in 0..250 {
            app.push_event(ReasoningEvent::FactExtracted {
                content: format!("fact {}", i),
            });
        }
        assert_eq!(app.consolidation_log.len(), 200);
    }
}
