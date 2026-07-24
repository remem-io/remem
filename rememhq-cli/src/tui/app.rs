//! Application state and update logic for the TUI.

use std::collections::{HashSet, VecDeque};

use rememhq_core::memory::types::{
    KnowledgeGraphUpdate, MemoryRecord, MemoryResult, MemoryType, MemoryVersionRecord,
    SessionSummaryRecord,
};
use rememhq_core::reasoning::ReasoningEvent;
use rememhq_core::storage::StoreStats;
use uuid::Uuid;

/// A reasoning event wrapped with its arrival timestamp.
#[derive(Debug, Clone)]
pub struct TimestampedEvent {
    pub event: ReasoningEvent,
    pub received_at: std::time::Instant,
}

/// Classify a ReasoningEvent into a short type label for counters.
pub fn event_type_label(event: &ReasoningEvent) -> &'static str {
    match event {
        ReasoningEvent::ConsolidationStarted { .. } => "start",
        ReasoningEvent::FactExtracted { .. } => "fact",
        ReasoningEvent::ContradictionDetected { .. } => "clash",
        ReasoningEvent::KnowledgeTripleFound { .. } => "graph",
        ReasoningEvent::ConsolidationCompleted { .. } => "done",
        ReasoningEvent::ThinkingDelta { .. } => "think",
        ReasoningEvent::ToolCall { .. } => "tool",
        ReasoningEvent::ObservationStreamed { .. } => "obs",
        ReasoningEvent::MemoryRecalled { .. } => "recall",
    }
}

/// Detect AI agent tool/framework from session ID (plain text without emojis).
pub fn detect_agent_name(session_id: &str) -> &'static str {
    let lower = session_id.to_lowercase();
    if lower.contains("claude") {
        "Claude Code"
    } else if lower.contains("codex") {
        "Codex"
    } else if lower.contains("antigravity") || lower.contains("gemini") {
        "Antigravity CLI"
    } else if lower.contains("cursor") {
        "Cursor"
    } else if lower.contains("copilot") {
        "GitHub Copilot"
    } else if lower.contains("opencode") {
        "OpenCode"
    } else if lower.contains("aider") {
        "Aider"
    } else if lower.contains("windsurf") {
        "Windsurf"
    } else if lower.contains("roo") {
        "Roo Code"
    } else if lower.contains("cline") {
        "Cline"
    } else {
        "AI Agent"
    }
}

/// Which pane / input mode / modal overlay the TUI is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Default: memory list focused.
    Browse,
    /// Filter/search input has focus.
    SearchInput,
    /// Guided recall query input (`:recall`).
    RecallInput,
    /// Guided recall results view.
    RecallResults,
    /// Knowledge Graph & Entity Browser view.
    GraphBrowser,
    /// Session Transcript & Summary Viewer view.
    SessionViewer,
    /// Detail pane for the selected memory is focused.
    Detail,
    /// Stats dashboard pane is focused.
    Stats,
    /// Consolidation monitor pane active.
    Monitor,
    /// Confirmation dialog for destructive actions (archive, delete, decay).
    ConfirmModal,
    /// Inline Memory Editor modal overlay.
    EditModal,
    /// In-TUI New Memory creation modal overlay.
    NewMemoryModal,
    /// Memory Version History & Diff Viewer view.
    VersionHistory,
    /// Help cheat-sheet overlay.
    HelpModal,
}

/// Generalized action target for the confirmation modal.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    Archive(Uuid),
    Delete(Uuid),
    Decay(Uuid, f32),
    BulkArchive(Vec<Uuid>),
    BulkDelete(Vec<Uuid>),
    BulkDecay(Vec<Uuid>, f32),
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
            SortField::CreatedAt => write!(f, "created_at"),
        }
    }
}

/// Active MemoryType filter for the browser table.
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
    /// Cycle to the next type filter.
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

    /// Convert to `Option<MemoryType>` for store queries.
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
    /// Multi-selected memory UUID set.
    pub selected_set: HashSet<Uuid>,
    /// Results from guided LLM recall query.
    pub recall_results: Vec<MemoryResult>,
    /// Selected index in recall_results view.
    pub recall_selected: usize,
    /// Knowledge graph triples.
    pub graph_triples: Vec<KnowledgeGraphUpdate>,
    /// Selected index in Knowledge Graph view.
    pub graph_selected: usize,
    /// Session summary records.
    pub session_summaries: Vec<SessionSummaryRecord>,
    /// Selected index in Session Viewer.
    pub session_selected: usize,
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
    /// Executed command history for `:` command palette.
    pub command_history: Vec<String>,
    /// Active index during command history navigation (`↑`/`↓`).
    pub command_history_idx: Option<usize>,
    /// Consolidation event log ring buffer with timestamps.
    pub consolidation_log: VecDeque<TimestampedEvent>,
    /// Monitor scroll offset (0 = bottom / newest).
    pub monitor_scroll: usize,
    /// Whether the monitor auto-scrolls to the latest event.
    pub monitor_auto_scroll: bool,
    /// Timestamps of recent events for calculating events/sec rate.
    pub monitor_event_times: VecDeque<std::time::Instant>,
    /// Per-event-type counters for the header.
    pub monitor_type_counts: std::collections::HashMap<String, usize>,
    /// Sparkline data: events per tick window (last 30 ticks).
    pub monitor_sparkline: VecDeque<u64>,
    /// Tick counter for sparkline aggregation.
    pub monitor_tick_count: u64,
    /// Events received in current tick window.
    pub monitor_tick_events: u64,
    /// Active AI Agent sessions map: session_id -> (agent_name, last_active_time).
    pub active_agent_sessions:
        std::collections::HashMap<String, (&'static str, std::time::Instant)>,
    /// Latest store statistics.
    pub stats: Option<StoreStats>,
    /// Action targeted for confirmation modal.
    pub confirm_action: Option<ConfirmAction>,
    /// Memory record currently being edited in EditModal.
    pub edit_record: Option<MemoryRecord>,
    /// Editable string buffer for EditModal content.
    pub edit_content_input: String,
    /// Editable string buffer for EditModal importance.
    pub edit_importance_input: String,
    /// Editable string buffer for EditModal tags.
    pub edit_tags_input: String,
    /// Active field index in EditModal (0: content, 1: type, 2: importance, 3: tags).
    pub edit_focus_field: usize,
    /// New memory modal buffers.
    pub new_content: String,
    pub new_type: MemoryType,
    pub new_importance: String,
    pub new_tags: String,
    /// Active field index in NewMemoryModal (0: content, 1: type, 2: importance, 3: tags).
    pub new_focus_field: usize,
    /// Version history records for selected memory.
    pub version_history: Vec<MemoryVersionRecord>,
    /// Selected index in version history view.
    pub version_selected: usize,
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
    /// Pagination: current zero-indexed page.
    pub page: usize,
    /// Pagination: records per page.
    pub page_size: usize,
    /// Total count of matching memories in store.
    pub total_count: usize,
    /// Whether to include archived memories in list view.
    pub show_archived: bool,
}

impl App {
    /// Create a new App with sensible defaults.
    pub fn new() -> Self {
        Self {
            mode: Mode::Browse,
            previous_mode: Mode::Browse,
            memories: Vec::new(),
            selected_set: HashSet::new(),
            recall_results: Vec::new(),
            recall_selected: 0,
            graph_triples: Vec::new(),
            graph_selected: 0,
            session_summaries: Vec::new(),
            session_selected: 0,
            selected: 0,
            filter_input: String::new(),
            filter_cursor: 0,
            recall_input: String::new(),
            recall_cursor: 0,
            command_history: Vec::new(),
            command_history_idx: None,
            consolidation_log: VecDeque::with_capacity(500),
            monitor_scroll: 0,
            monitor_auto_scroll: true,
            monitor_event_times: VecDeque::with_capacity(200),
            monitor_type_counts: std::collections::HashMap::new(),
            monitor_sparkline: VecDeque::with_capacity(30),
            monitor_tick_count: 0,
            monitor_tick_events: 0,
            active_agent_sessions: std::collections::HashMap::new(),
            stats: None,
            confirm_action: None,
            edit_record: None,
            edit_content_input: String::new(),
            edit_importance_input: String::new(),
            edit_tags_input: String::new(),
            edit_focus_field: 0,
            new_content: String::new(),
            new_type: MemoryType::Fact,
            new_importance: "5.0".to_string(),
            new_tags: String::new(),
            new_focus_field: 0,
            version_history: Vec::new(),
            version_selected: 0,
            status_message: None,
            loading: false,
            should_quit: false,
            sort_field: SortField::Importance,
            sort_ascending: false,
            type_filter: TypeFilter::All,
            detail_scroll: 0,
            page: 0,
            page_size: 50,
            total_count: 0,
            show_archived: false,
        }
    }

    /// Set a status message that will expire after 4 seconds.
    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status_message = Some((text.into(), std::time::Instant::now()));
    }

    /// Return the active status message if it hasn't expired.
    pub fn active_status(&self) -> Option<&str> {
        if let Some((msg, created)) = &self.status_message {
            if created.elapsed() < std::time::Duration::from_secs(4) {
                return Some(msg.as_str());
            }
        }
        None
    }

    /// Toggle selection status of the currently highlighted memory row.
    pub fn toggle_selection(&mut self) {
        if let Some(mem) = self.selected_memory() {
            let id = mem.id;
            if self.selected_set.contains(&id) {
                self.selected_set.remove(&id);
            } else {
                self.selected_set.insert(id);
            }
        }
    }

    /// Select all memory records on the current page.
    #[allow(dead_code)]
    pub fn select_all_page(&mut self) {
        for mem in &self.memories {
            self.selected_set.insert(mem.id);
        }
    }

    /// Clear all multi-selections.
    pub fn clear_selections(&mut self) {
        self.selected_set.clear();
    }

    /// Cycle pane focus: Browse -> Detail -> Stats -> Monitor -> Browse.
    pub fn next_pane(&mut self) {
        self.mode = match self.mode {
            Mode::Browse => Mode::Detail,
            Mode::Detail => Mode::Stats,
            Mode::Stats => Mode::Monitor,
            Mode::Monitor => Mode::Browse,
            Mode::RecallResults | Mode::GraphBrowser | Mode::SessionViewer => Mode::Browse,
            _ => Mode::Browse,
        };
    }

    /// Open EditModal prefilled with the currently selected memory record.
    pub fn open_edit_modal(&mut self) {
        if let Some(mem) = self.selected_memory().cloned() {
            self.edit_content_input = mem.content.clone();
            self.edit_importance_input = format!("{:.1}", mem.importance);
            self.edit_tags_input = mem.tags.join(", ");
            self.edit_record = Some(mem);
            self.edit_focus_field = 0;
            self.previous_mode = self.mode;
            self.mode = Mode::EditModal;
        }
    }

    /// Open NewMemoryModal prefilled with defaults.
    pub fn open_new_memory_modal(&mut self) {
        self.new_content.clear();
        self.new_type = MemoryType::Fact;
        self.new_importance = "5.0".to_string();
        self.new_tags.clear();
        self.new_focus_field = 0;
        self.previous_mode = self.mode;
        self.mode = Mode::NewMemoryModal;
    }

    /// Currently selected MemoryRecord, if any.
    pub fn selected_memory(&self) -> Option<&MemoryRecord> {
        self.memories.get(self.selected)
    }

    /// Currently selected MemoryResult from guided recall, if any.
    pub fn selected_recall_result(&self) -> Option<&MemoryResult> {
        self.recall_results.get(self.recall_selected)
    }

    /// Select next item in browser.
    pub fn select_next(&mut self) {
        if !self.memories.is_empty() && self.selected < self.memories.len() - 1 {
            self.selected += 1;
        }
        self.detail_scroll = 0;
    }

    /// Select previous item in browser.
    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
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
        if self.selected >= self.memories.len() && !self.memories.is_empty() {
            self.selected = self.memories.len() - 1;
        }
    }

    /// Push a consolidation event into the ring buffer with timestamp tracking.
    pub fn push_event(&mut self, event: ReasoningEvent) {
        let now = std::time::Instant::now();

        // Extract session_id and update active agent tracker
        let sid_opt = match &event {
            ReasoningEvent::ConsolidationStarted { session_id } => Some(session_id.as_str()),
            ReasoningEvent::ConsolidationCompleted { session_id, .. } => Some(session_id.as_str()),
            ReasoningEvent::ThinkingDelta { session_id, .. } => Some(session_id.as_str()),
            ReasoningEvent::ToolCall { session_id, .. } => Some(session_id.as_str()),
            ReasoningEvent::ObservationStreamed { session_id, .. } => Some(session_id.as_str()),
            ReasoningEvent::MemoryRecalled { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        };
        if let Some(sid) = sid_opt {
            let agent_name = detect_agent_name(sid);
            self.active_agent_sessions
                .insert(sid.to_string(), (agent_name, now));
        }

        // Track per-type counter
        let label = event_type_label(&event).to_string();
        *self.monitor_type_counts.entry(label).or_insert(0) += 1;

        // Track event arrival time for rate calculation
        self.monitor_event_times.push_back(now);
        // Keep only events from the last 10 seconds
        while let Some(front) = self.monitor_event_times.front() {
            if now.duration_since(*front).as_secs() > 10 {
                self.monitor_event_times.pop_front();
            } else {
                break;
            }
        }

        // Sparkline: count events in this tick window
        self.monitor_tick_events += 1;

        // Store timestamped event
        let ts_event = TimestampedEvent {
            event,
            received_at: now,
        };
        if self.consolidation_log.len() >= 500 {
            self.consolidation_log.pop_front();
        }
        self.consolidation_log.push_back(ts_event);

        // Auto-scroll to bottom when enabled
        if self.monitor_auto_scroll {
            self.monitor_scroll = 0;
        }
    }

    /// Advance the sparkline tick (called from the Tick event handler).
    pub fn tick_sparkline(&mut self) {
        self.monitor_tick_count += 1;
        self.monitor_sparkline.push_back(self.monitor_tick_events);
        self.monitor_tick_events = 0;
        if self.monitor_sparkline.len() > 30 {
            self.monitor_sparkline.pop_front();
        }
    }

    /// Compute the current event rate (events per second over the last 10 seconds).
    pub fn event_rate(&self) -> f64 {
        if self.monitor_event_times.is_empty() {
            return 0.0;
        }
        let window_secs = 10.0;
        self.monitor_event_times.len() as f64 / window_secs
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
    fn push_event_caps_at_500() {
        let mut app = App::new();
        for i in 0..600 {
            app.push_event(ReasoningEvent::FactExtracted {
                content: format!("fact {}", i),
            });
        }
        assert_eq!(app.consolidation_log.len(), 500);
    }

    #[test]
    fn multi_select_toggle() {
        let mut app = App::new();
        let mem = MemoryRecord::new("Test", MemoryType::Fact);
        app.memories.push(mem.clone());
        app.toggle_selection();
        assert!(app.selected_set.contains(&mem.id));
        app.toggle_selection();
        assert!(!app.selected_set.contains(&mem.id));
    }
}
