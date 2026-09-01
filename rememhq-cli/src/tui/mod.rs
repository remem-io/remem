//! Terminal User Interface module for `remem`.
//!
//! Provides interactive memory browsing, full-text search, guided LLM recall,
//! inline memory editing, new memory creation, version history timeline viewing,
//! Knowledge Graph entity browsing, Session transcript viewing, multi-select bulk operations,
//! statistics visualization, and live inter-process event streaming using `ratatui`.

pub mod app;
pub mod data;
pub mod event;
pub mod ui;

use std::io::{stdout, Stdout};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use rememhq_core::memory::types::{MemoryRecord, MemoryType};
use rememhq_core::reasoning::ReasoningEngine;
use tokio::sync::mpsc;

use app::{App, ConfirmAction, Mode};
use event::{AppEvent, FetchResult};

/// Entry point for running the TUI.
pub async fn run_tui(
    engine: ReasoningEngine,
    _config: &rememhq_core::config::RememConfig,
) -> anyhow::Result<()> {
    run(engine).await
}

/// Entry point for running the TUI.
pub async fn run(engine: ReasoningEngine) -> anyhow::Result<()> {
    setup_panic_hook();

    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, engine).await;

    restore_terminal(&mut terminal)?;

    if let Err(ref e) = result {
        eprintln!("TUI Error: {:?}", e);
    }

    result
}

/// Set up a panic hook to clean up the terminal raw mode before printing panic info.
fn setup_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));
}

/// Initialize terminal raw mode and alternate screen.
fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore terminal to original state.
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Main application event loop.
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    engine: ReasoningEngine,
) -> anyhow::Result<()> {
    let mut app = App::new();

    let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchResult>();
    let store = engine.store.clone();
    let engine_arc = Arc::new(engine);

    // Initial data fetch using list_paged
    data::spawn_list_paged_fetch(
        store.clone(),
        fetch_tx.clone(),
        app.type_filter.to_memory_type(),
        app.page,
        app.page_size,
        app.show_archived,
    );
    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
    data::spawn_telemetry_fetch(engine_arc.clone(), fetch_tx.clone());
    data::spawn_event_tailer(
        engine_arc.config.project_data_dir().join("events.jsonl"),
        fetch_tx.clone(),
    );

    let mut event_stream = EventStream::new();
    let mut engine_rx = engine_arc.event_bus.subscribe();
    let mut tick_interval = tokio::time::interval(Duration::from_millis(500));

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if app.should_quit {
            break;
        }

        tokio::select! {
            // Crossterm keyboard / terminal events
            maybe_event = event_stream.next() => {
                if let Some(Ok(event)) = maybe_event {
                    handle_event(&mut app, AppEvent::Input(event), &store, &engine_arc, &fetch_tx);
                }
            }

            // Engine events (consolidation, facts, contradictions)
            Ok(reasoning_event) = engine_rx.recv() => {
                handle_event(&mut app, AppEvent::Reasoning(reasoning_event), &store, &engine_arc, &fetch_tx);
            }

            // Periodic tick (refresh stats)
            _ = tick_interval.tick() => {
                handle_event(&mut app, AppEvent::Tick, &store, &engine_arc, &fetch_tx);
            }

            // Background fetch completion
            Some(fetch_result) = fetch_rx.recv() => {
                handle_event(&mut app, AppEvent::FetchComplete(fetch_result), &store, &engine_arc, &fetch_tx);
            }
        }
    }

    Ok(())
}

/// Handle a single application event.
fn handle_event(
    app: &mut App,
    event: AppEvent,
    store: &Arc<rememhq_core::storage::sqlite::SqliteStore>,
    engine: &Arc<ReasoningEngine>,
    fetch_tx: &mpsc::UnboundedSender<FetchResult>,
) {
    match event {
        AppEvent::Input(Event::Key(key)) => {
            // Global quit on Ctrl-C
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
                return;
            }

            match app.mode {
                Mode::HelpModal => match key.code {
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('q') => {
                        app.mode = app.previous_mode;
                    }
                    _ => {}
                },

                Mode::ConfirmModal => match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        if let Some(action) = app.confirm_action.take() {
                            match action {
                                ConfirmAction::Archive(id) => {
                                    app.set_status(format!(
                                        "Archiving memory {}...",
                                        &id.to_string()[..8]
                                    ));
                                    data::spawn_archive_task(store.clone(), fetch_tx.clone(), id);
                                }
                                ConfirmAction::Delete(id) => {
                                    app.set_status(format!(
                                        "Deleting memory {}...",
                                        &id.to_string()[..8]
                                    ));
                                    data::spawn_delete_task(store.clone(), fetch_tx.clone(), id);
                                }
                                ConfirmAction::Decay(_id, factor) => {
                                    app.set_status(format!(
                                        "Applying decay factor {:.2}...",
                                        factor
                                    ));
                                    data::spawn_decay_task(store.clone(), fetch_tx.clone(), factor);
                                }
                                ConfirmAction::BulkArchive(ids) => {
                                    app.set_status(format!(
                                        "Bulk archiving {} memories...",
                                        ids.len()
                                    ));
                                    data::spawn_bulk_archive_task(
                                        store.clone(),
                                        fetch_tx.clone(),
                                        ids,
                                    );
                                }
                                ConfirmAction::BulkDelete(ids) => {
                                    app.set_status(format!(
                                        "Bulk deleting {} memories...",
                                        ids.len()
                                    ));
                                    data::spawn_bulk_delete_task(
                                        store.clone(),
                                        fetch_tx.clone(),
                                        ids,
                                    );
                                }
                                ConfirmAction::BulkDecay(ids, factor) => {
                                    app.set_status(format!(
                                        "Bulk decaying {} memories...",
                                        ids.len()
                                    ));
                                    data::spawn_decay_task(store.clone(), fetch_tx.clone(), factor);
                                }
                            }
                        }
                        app.mode = app.previous_mode;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        app.confirm_action = None;
                        app.mode = app.previous_mode;
                    }
                    _ => {}
                },

                Mode::EditModal => match key.code {
                    KeyCode::Esc => {
                        app.edit_record = None;
                        app.mode = app.previous_mode;
                    }
                    KeyCode::Tab => {
                        app.edit_focus_field = (app.edit_focus_field + 1) % 4;
                    }
                    KeyCode::BackTab => {
                        app.edit_focus_field = (app.edit_focus_field + 3) % 4;
                    }
                    KeyCode::Enter => {
                        if let Some(mut record) = app.edit_record.take() {
                            record.content = app.edit_content_input.clone();
                            if let Ok(imp) = app.edit_importance_input.parse::<f32>() {
                                record.importance = imp.clamp(1.0, 10.0);
                            }
                            record.tags = app
                                .edit_tags_input
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            app.set_status(format!(
                                "Saving memory record {}...",
                                &record.id.to_string()[..8]
                            ));
                            data::spawn_update_task(store.clone(), fetch_tx.clone(), record);
                        }
                        app.mode = app.previous_mode;
                    }
                    KeyCode::Char('t') | KeyCode::Char(' ') if app.edit_focus_field == 1 => {
                        if let Some(ref mut rec) = app.edit_record {
                            rec.memory_type = match rec.memory_type {
                                MemoryType::Fact => MemoryType::Procedure,
                                MemoryType::Procedure => MemoryType::Preference,
                                MemoryType::Preference => MemoryType::Decision,
                                MemoryType::Decision => MemoryType::Observation,
                                MemoryType::Observation => MemoryType::Fact,
                            };
                        }
                    }
                    KeyCode::Backspace => match app.edit_focus_field {
                        0 => {
                            app.edit_content_input.pop();
                        }
                        2 => {
                            app.edit_importance_input.pop();
                        }
                        3 => {
                            app.edit_tags_input.pop();
                        }
                        _ => {}
                    },
                    KeyCode::Char(c) => match app.edit_focus_field {
                        0 => app.edit_content_input.push(c),
                        2 => app.edit_importance_input.push(c),
                        3 => app.edit_tags_input.push(c),
                        _ => {}
                    },
                    _ => {}
                },

                Mode::NewMemoryModal => match key.code {
                    KeyCode::Esc => {
                        app.mode = app.previous_mode;
                    }
                    KeyCode::Tab => {
                        app.new_focus_field = (app.new_focus_field + 1) % 4;
                    }
                    KeyCode::BackTab => {
                        app.new_focus_field = (app.new_focus_field + 3) % 4;
                    }
                    KeyCode::Enter => {
                        let content = app.new_content.trim().to_string();
                        if !content.is_empty() {
                            let mut record = MemoryRecord::new(content, app.new_type);
                            if let Ok(imp) = app.new_importance.parse::<f32>() {
                                record.importance = imp.clamp(1.0, 10.0);
                            }
                            record.tags = app
                                .new_tags
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();

                            app.set_status("Creating new memory record...");
                            data::spawn_create_task(store.clone(), fetch_tx.clone(), record);
                        }
                        app.mode = app.previous_mode;
                    }
                    KeyCode::Char('t') | KeyCode::Char(' ') if app.new_focus_field == 1 => {
                        app.new_type = match app.new_type {
                            MemoryType::Fact => MemoryType::Procedure,
                            MemoryType::Procedure => MemoryType::Preference,
                            MemoryType::Preference => MemoryType::Decision,
                            MemoryType::Decision => MemoryType::Observation,
                            MemoryType::Observation => MemoryType::Fact,
                        };
                    }
                    KeyCode::Backspace => match app.new_focus_field {
                        0 => {
                            app.new_content.pop();
                        }
                        2 => {
                            app.new_importance.pop();
                        }
                        3 => {
                            app.new_tags.pop();
                        }
                        _ => {}
                    },
                    KeyCode::Char(c) => match app.new_focus_field {
                        0 => app.new_content.push(c),
                        2 => app.new_importance.push(c),
                        3 => app.new_tags.push(c),
                        _ => {}
                    },
                    _ => {}
                },

                Mode::GraphBrowser => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.graph_selected > 0 {
                            app.graph_selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if app.graph_selected + 1 < app.graph_triples.len() =>
                    {
                        app.graph_selected += 1;
                    }
                    _ => {}
                },

                Mode::SessionViewer => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.session_selected > 0 {
                            app.session_selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if app.session_selected + 1 < app.session_summaries.len() =>
                    {
                        app.session_selected += 1;
                    }
                    KeyCode::Char('c') => {
                        let sid = app
                            .session_summaries
                            .get(app.session_selected)
                            .map(|s| s.session_id.clone());
                        if let Some(id) = sid {
                            app.set_status(format!("Consolidating session {}...", id));
                            data::spawn_consolidate_session_task(
                                engine.clone(),
                                fetch_tx.clone(),
                                id,
                            );
                        }
                    }
                    _ => {}
                },

                Mode::VersionHistory => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.version_selected > 0 {
                            app.version_selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if app.version_selected + 1 < app.version_history.len() =>
                    {
                        app.version_selected += 1;
                    }
                    _ => {}
                },

                Mode::SearchInput => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Browse;
                        app.filter_input.clear();
                        app.filter_cursor = 0;
                        app.loading = true;
                        data::spawn_list_paged_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            app.type_filter.to_memory_type(),
                            app.page,
                            app.page_size,
                            app.show_archived,
                        );
                    }
                    KeyCode::Enter => {
                        app.mode = Mode::Browse;
                    }
                    KeyCode::Backspace => {
                        app.filter_backspace();
                        trigger_live_search(app, store, fetch_tx);
                    }
                    KeyCode::Left => app.filter_cursor_left(),
                    KeyCode::Right => app.filter_cursor_right(),
                    KeyCode::Char(c) => {
                        app.filter_insert_char(c);
                        trigger_live_search(app, store, fetch_tx);
                    }
                    _ => {}
                },

                Mode::RecallInput => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Browse;
                        app.recall_input.clear();
                        app.recall_cursor = 0;
                        app.command_history_idx = None;
                    }
                    KeyCode::Up => {
                        if !app.command_history.is_empty() {
                            let next_idx = match app.command_history_idx {
                                Some(i) if i > 0 => i - 1,
                                Some(0) => 0,
                                None => app.command_history.len() - 1,
                                _ => 0,
                            };
                            app.command_history_idx = Some(next_idx);
                            app.recall_input = app.command_history[next_idx].clone();
                            app.recall_cursor = app.recall_input.len();
                        }
                    }
                    KeyCode::Down => {
                        if let Some(i) = app.command_history_idx {
                            if i + 1 < app.command_history.len() {
                                app.command_history_idx = Some(i + 1);
                                app.recall_input = app.command_history[i + 1].clone();
                                app.recall_cursor = app.recall_input.len();
                            } else {
                                app.command_history_idx = None;
                                app.recall_input.clear();
                                app.recall_cursor = 0;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        let raw_cmd = app.recall_input.trim().to_string();
                        if !raw_cmd.is_empty() {
                            app.command_history.push(raw_cmd.clone());
                            app.command_history_idx = None;
                            execute_command_palette(raw_cmd, app, store, engine, fetch_tx);
                        } else {
                            app.mode = Mode::Browse;
                        }
                    }
                    KeyCode::Backspace => app.recall_backspace(),
                    KeyCode::Left => app.recall_cursor_left(),
                    KeyCode::Right => app.recall_cursor_right(),
                    KeyCode::Char(c) => app.recall_insert_char(c),
                    _ => {}
                },

                Mode::RecallResults => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.recall_selected > 0 {
                            app.recall_selected -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if app.recall_selected + 1 < app.recall_results.len() =>
                    {
                        app.recall_selected += 1;
                    }
                    KeyCode::Enter => {
                        if let Some(res) = app.selected_recall_result() {
                            app.set_status(format!(
                                "Inspecting recall result {}",
                                &res.id.to_string()[..8]
                            ));
                            app.mode = Mode::Detail;
                        }
                    }
                    _ => {}
                },

                Mode::Detail => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Tab => app.next_pane(),
                    KeyCode::BackTab => app.next_pane(),
                    KeyCode::Down | KeyCode::Char('j') => app.detail_scroll += 1,
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.detail_scroll = app.detail_scroll.saturating_sub(1);
                    }
                    KeyCode::PageDown => app.detail_scroll += 10,
                    KeyCode::PageUp => {
                        app.detail_scroll = app.detail_scroll.saturating_sub(10);
                    }
                    KeyCode::Home => app.detail_scroll = 0,
                    KeyCode::Char('e') => {
                        app.open_edit_modal();
                    }
                    KeyCode::Char('v') => {
                        let target_id = app.selected_memory().map(|m| m.id);
                        if let Some(id) = target_id {
                            app.set_status("Fetching memory version history...");
                            data::spawn_versions_fetch(store.clone(), fetch_tx.clone(), id);
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Char('a') => {
                        if let Some(mem) = app.selected_memory() {
                            app.confirm_action = Some(ConfirmAction::Archive(mem.id));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Char('u') => {
                        let target_id = app.selected_memory().map(|m| m.id);
                        if let Some(id) = target_id {
                            app.set_status(format!(
                                "Unarchiving memory {}...",
                                &id.to_string()[..8]
                            ));
                            data::spawn_unarchive_task(store.clone(), fetch_tx.clone(), id);
                        }
                    }
                    _ => {}
                },

                Mode::Browse => match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Tab => app.next_pane(),
                    KeyCode::BackTab => app.next_pane(),
                    KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Char('s') => app.mode = Mode::Stats,
                    KeyCode::Char('m') => app.mode = Mode::Monitor,
                    KeyCode::Char('T') | KeyCode::Char('7') => {
                        app.mode = Mode::Telemetry;
                        data::spawn_telemetry_fetch(engine.clone(), fetch_tx.clone());
                    }

                    KeyCode::Char('g') => {
                        app.set_status("Fetching Knowledge Graph triples...");
                        data::spawn_graph_fetch(store.clone(), fetch_tx.clone());
                    }
                    KeyCode::Char('S') => {
                        app.set_status("Fetching session summaries...");
                        data::spawn_sessions_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            "default".to_string(),
                        );
                    }
                    KeyCode::Char(' ') => {
                        app.toggle_selection();
                    }
                    KeyCode::Char('A') => {
                        if !app.selected_set.is_empty() {
                            let ids: Vec<_> = app.selected_set.iter().copied().collect();
                            app.confirm_action = Some(ConfirmAction::BulkArchive(ids));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Char('X') => {
                        if !app.selected_set.is_empty() {
                            let ids: Vec<_> = app.selected_set.iter().copied().collect();
                            app.confirm_action = Some(ConfirmAction::BulkDelete(ids));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Char('C') => {
                        if !app.selected_set.is_empty() {
                            let ids: Vec<_> = app.selected_set.iter().copied().collect();
                            app.confirm_action = Some(ConfirmAction::BulkDecay(ids, 0.5));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Esc if !app.selected_set.is_empty() => {
                        app.clear_selections();
                        app.set_status("Cleared selections");
                    }

                    KeyCode::Char('n') => {
                        app.open_new_memory_modal();
                    }
                    KeyCode::Char('e') => {
                        app.open_edit_modal();
                    }
                    KeyCode::Char('v') => {
                        let target_id = app.selected_memory().map(|m| m.id);
                        if let Some(id) = target_id {
                            app.set_status("Fetching memory version history...");
                            data::spawn_versions_fetch(store.clone(), fetch_tx.clone(), id);
                        }
                    }

                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                    KeyCode::Home => app.select_first(),
                    KeyCode::End => app.select_last(),

                    // Pagination navigation
                    KeyCode::Left | KeyCode::Char('[') => {
                        if app.page > 0 {
                            app.page -= 1;
                            app.selected = 0;
                            app.loading = true;
                            data::spawn_list_paged_fetch(
                                store.clone(),
                                fetch_tx.clone(),
                                app.type_filter.to_memory_type(),
                                app.page,
                                app.page_size,
                                app.show_archived,
                            );
                        }
                    }
                    KeyCode::Right | KeyCode::Char(']') => {
                        if (app.page + 1) * app.page_size < app.total_count {
                            app.page += 1;
                            app.selected = 0;
                            app.loading = true;
                            data::spawn_list_paged_fetch(
                                store.clone(),
                                fetch_tx.clone(),
                                app.type_filter.to_memory_type(),
                                app.page,
                                app.page_size,
                                app.show_archived,
                            );
                        }
                    }

                    KeyCode::Enter => {
                        if !app.memories.is_empty() {
                            app.mode = Mode::Detail;
                        }
                    }
                    KeyCode::Char('/') => {
                        app.mode = Mode::SearchInput;
                    }
                    KeyCode::Char(':') => {
                        app.mode = Mode::RecallInput;
                    }
                    KeyCode::Char('t') => {
                        app.type_filter = app.type_filter.next();
                        app.page = 0;
                        app.loading = true;
                        data::spawn_list_paged_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            app.type_filter.to_memory_type(),
                            app.page,
                            app.page_size,
                            app.show_archived,
                        );
                    }
                    KeyCode::Char('d') | KeyCode::Char('a') => {
                        if let Some(mem) = app.selected_memory() {
                            app.confirm_action = Some(ConfirmAction::Archive(mem.id));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Char('D') | KeyCode::Char('x') => {
                        if let Some(mem) = app.selected_memory() {
                            app.confirm_action = Some(ConfirmAction::Delete(mem.id));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Char('u') => {
                        let target_id = app.selected_memory().map(|m| m.id);
                        if let Some(id) = target_id {
                            app.set_status(format!(
                                "Unarchiving memory {}...",
                                &id.to_string()[..8]
                            ));
                            data::spawn_unarchive_task(store.clone(), fetch_tx.clone(), id);
                        }
                    }
                    KeyCode::Char('c') => {
                        if let Some(mem) = app.selected_memory() {
                            app.confirm_action = Some(ConfirmAction::Decay(mem.id, 0.5));
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmModal;
                        }
                    }
                    KeyCode::Char('o') => {
                        app.sort_field = app.sort_field.next();
                        app.sort_memories();
                    }
                    KeyCode::Char('O') => {
                        app.sort_ascending = !app.sort_ascending;
                        app.sort_memories();
                    }
                    KeyCode::Char('r') => {
                        app.loading = true;
                        app.set_status("Refreshed store records & stats");
                        data::spawn_list_paged_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            app.type_filter.to_memory_type(),
                            app.page,
                            app.page_size,
                            app.show_archived,
                        );
                        data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                    }
                    KeyCode::Char('?') | KeyCode::Char('h') => {
                        app.previous_mode = app.mode;
                        app.mode = Mode::HelpModal;
                    }
                    _ => {}
                },

                Mode::Stats => match key.code {
                    KeyCode::Tab => app.next_pane(),
                    KeyCode::BackTab => app.next_pane(),
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('b') => {
                        app.mode = Mode::Browse
                    }
                    KeyCode::Char('?') | KeyCode::Char('h') => {
                        app.previous_mode = app.mode;
                        app.mode = Mode::HelpModal;
                    }
                    _ => {}
                },

                Mode::Monitor => match key.code {
                    KeyCode::Tab => app.next_pane(),
                    KeyCode::BackTab => app.next_pane(),
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('b') => {
                        app.mode = Mode::Browse
                    }
                    KeyCode::Char('?') | KeyCode::Char('h') => {
                        app.previous_mode = app.mode;
                        app.mode = Mode::HelpModal;
                    }
                    // Scroll up (older events)
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.monitor_auto_scroll = false;
                        let max_scroll = app.consolidation_log.len().saturating_sub(1);
                        app.monitor_scroll = (app.monitor_scroll + 1).min(max_scroll);
                    }
                    // Scroll down (newer events)
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.monitor_scroll > 0 {
                            app.monitor_scroll -= 1;
                        }
                        if app.monitor_scroll == 0 {
                            app.monitor_auto_scroll = true;
                        }
                    }
                    KeyCode::PageUp => {
                        app.monitor_auto_scroll = false;
                        let max_scroll = app.consolidation_log.len().saturating_sub(1);
                        app.monitor_scroll = (app.monitor_scroll + 10).min(max_scroll);
                    }
                    KeyCode::PageDown => {
                        app.monitor_scroll = app.monitor_scroll.saturating_sub(10);
                        if app.monitor_scroll == 0 {
                            app.monitor_auto_scroll = true;
                        }
                    }
                    // Pause/resume auto-scroll
                    KeyCode::Char('p') => {
                        app.monitor_auto_scroll = !app.monitor_auto_scroll;
                        if app.monitor_auto_scroll {
                            app.monitor_scroll = 0;
                        }
                    }
                    // Jump to latest (resume live)
                    KeyCode::Char('G') => {
                        app.monitor_scroll = 0;
                        app.monitor_auto_scroll = true;
                    }
                    _ => {}
                },

                Mode::Telemetry => match key.code {
                    KeyCode::Esc | KeyCode::Char('b') => app.mode = Mode::Browse,
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Char('r') => {
                        app.set_status("Refreshing telemetry & token cost metrics...");
                        data::spawn_telemetry_fetch(engine.clone(), fetch_tx.clone());
                    }
                    _ => {}
                },
            }
        }
        AppEvent::Input(_) => {}

        AppEvent::Reasoning(reasoning_event) => {
            app.push_event(reasoning_event);
        }

        AppEvent::Tick => {
            app.tick_sparkline();
            data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
            data::spawn_telemetry_fetch(engine.clone(), fetch_tx.clone());
        }

        AppEvent::FetchComplete(result) => match result {
            FetchResult::Memories(res) => {
                app.loading = false;
                if let Ok(memories) = res {
                    app.memories = memories;
                    app.sort_memories();
                }
            }
            FetchResult::PagedMemories(res) => {
                app.loading = false;
                if let Ok((memories, total_count)) = res {
                    app.memories = memories;
                    app.total_count = total_count;
                    app.sort_memories();
                }
            }
            FetchResult::Recall(res) => {
                app.loading = false;
                if let Ok(results) = res {
                    let count = results.len();
                    app.recall_results = results;
                    app.recall_selected = 0;
                    if count > 0 {
                        app.set_status(format!(
                            "Recall retrieved {} results — displaying in Recall view",
                            count
                        ));
                        app.mode = Mode::RecallResults;
                    } else {
                        app.set_status("Recall retrieved 0 results for query");
                        app.mode = Mode::Browse;
                    }
                }
            }
            FetchResult::Graph(res) => {
                app.loading = false;
                if let Ok(triples) = res {
                    let count = triples.len();
                    app.set_status(format!("Retrieved {} Knowledge Graph triples", count));
                    app.graph_triples = triples;
                    app.graph_selected = 0;
                    app.mode = Mode::GraphBrowser;
                } else {
                    app.set_status("Failed to fetch Knowledge Graph triples");
                }
            }
            FetchResult::Sessions(res) => {
                app.loading = false;
                if let Ok(sessions) = res {
                    let count = sessions.len();
                    app.set_status(format!("Retrieved {} session summaries", count));
                    app.session_summaries = sessions;
                    app.session_selected = 0;
                    app.mode = Mode::SessionViewer;
                } else {
                    app.set_status("Failed to fetch session summaries");
                }
            }
            FetchResult::Stats(res) => {
                if let Ok(stats) = res {
                    app.stats = Some(stats);
                }
            }
            FetchResult::Telemetry(res) => {
                if let Ok(telemetry) = res {
                    app.telemetry = Some(telemetry);
                }
            }
            FetchResult::Archived(id, res) => {
                if matches!(res, Ok(true)) {
                    app.set_status(format!(
                        "Successfully archived memory {}",
                        &id.to_string()[..8]
                    ));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to archive memory");
                }
            }
            FetchResult::Unarchived(id, res) => {
                if matches!(res, Ok(true)) {
                    app.set_status(format!(
                        "Successfully unarchived memory {}",
                        &id.to_string()[..8]
                    ));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to unarchive memory");
                }
            }
            FetchResult::Deleted(id, res) => {
                if matches!(res, Ok(true)) {
                    app.set_status(format!(
                        "Permanently deleted memory {}",
                        &id.to_string()[..8]
                    ));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to delete memory");
                }
            }
            FetchResult::Decayed(res) => {
                if let Ok(count) = res {
                    app.set_status(format!("Applied decay across {} memories", count));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to apply decay");
                }
            }
            FetchResult::BulkArchived(_ids, res) => {
                app.clear_selections();
                if let Ok(count) = res {
                    app.set_status(format!("Bulk archived {} memories", count));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to bulk archive memories");
                }
            }
            FetchResult::BulkDeleted(_ids, res) => {
                app.clear_selections();
                if let Ok(count) = res {
                    app.set_status(format!("Bulk deleted {} memories", count));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to bulk delete memories");
                }
            }
            FetchResult::ConsolidatedSession(sid, res) => {
                if let Ok(report) = res {
                    app.set_status(format!(
                        "Consolidated session {}: extracted {} new facts",
                        sid, report.new_facts
                    ));
                    data::spawn_sessions_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        "default".to_string(),
                    );
                } else {
                    app.set_status(format!("Failed to consolidate session {}", sid));
                }
            }
            FetchResult::Updated(id, res) => {
                if res.is_ok() {
                    app.set_status(format!("Updated memory record {}", &id.to_string()[..8]));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to update memory record");
                }
            }
            FetchResult::Created(res) => {
                if let Ok(rec) = res {
                    app.set_status(format!(
                        "Created new memory record {}",
                        &rec.id.to_string()[..8]
                    ));
                    data::spawn_list_paged_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        app.page,
                        app.page_size,
                        app.show_archived,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to create memory record");
                }
            }
            FetchResult::Versions(id, res) => {
                app.loading = false;
                if let Ok(history) = res {
                    let count = history.len();
                    app.set_status(format!(
                        "Retrieved {} history revisions for {}",
                        count,
                        &id.to_string()[..8]
                    ));
                    app.version_history = history;
                    app.version_selected = 0;
                    app.mode = Mode::VersionHistory;
                } else {
                    app.set_status("Failed to fetch version history");
                }
            }
            FetchResult::LiveEvent(event) => {
                let needs_refresh = matches!(
                    event,
                    rememhq_core::reasoning::ReasoningEvent::FactExtracted { .. }
                        | rememhq_core::reasoning::ReasoningEvent::ContradictionDetected { .. }
                        | rememhq_core::reasoning::ReasoningEvent::KnowledgeTripleFound { .. }
                        | rememhq_core::reasoning::ReasoningEvent::ConsolidationCompleted { .. }
                );
                app.push_event(event);
                if needs_refresh {
                    trigger_live_search(app, store, fetch_tx);
                    if app.mode == Mode::GraphBrowser {
                        data::spawn_graph_fetch(store.clone(), fetch_tx.clone());
                    } else if app.mode == Mode::SessionViewer {
                        data::spawn_sessions_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            engine.config.project.clone(),
                        );
                    }
                }
            }
        },
    }
}

/// Trigger live search-as-you-type based on current filter_input.
fn trigger_live_search(
    app: &mut App,
    store: &Arc<rememhq_core::storage::sqlite::SqliteStore>,
    fetch_tx: &mpsc::UnboundedSender<FetchResult>,
) {
    if app.filter_input.trim().is_empty() {
        data::spawn_list_paged_fetch(
            store.clone(),
            fetch_tx.clone(),
            app.type_filter.to_memory_type(),
            app.page,
            app.page_size,
            app.show_archived,
        );
    } else {
        data::spawn_search_fetch(
            store.clone(),
            fetch_tx.clone(),
            app.filter_input.clone(),
            100,
        );
    }
}

/// Execute a command string entered into the Universal Command Palette (`:` prompt).
fn execute_command_palette(
    cmd: String,
    app: &mut App,
    store: &Arc<rememhq_core::storage::sqlite::SqliteStore>,
    engine: &Arc<ReasoningEngine>,
    fetch_tx: &mpsc::UnboundedSender<FetchResult>,
) {
    let lower = cmd.trim().to_lowercase();
    if lower == ":q" || lower == ":quit" || lower == "quit" {
        app.should_quit = true;
    } else if lower == ":h" || lower == ":help" || lower == "help" {
        app.previous_mode = app.mode;
        app.mode = Mode::HelpModal;
    } else if lower == ":clear" || lower == "clear" {
        app.filter_input.clear();
        app.filter_cursor = 0;
        app.type_filter = app::TypeFilter::All;
        app.set_status("Cleared all filters and search input");
        app.mode = Mode::Browse;
        data::spawn_list_paged_fetch(
            store.clone(),
            fetch_tx.clone(),
            app.type_filter.to_memory_type(),
            app.page,
            app.page_size,
            app.show_archived,
        );
    } else if lower.starts_with(":filter ") || lower.starts_with(":f ") {
        let parts: Vec<_> = cmd.split_whitespace().collect();
        if parts.len() > 1 {
            match parts[1].to_lowercase().as_str() {
                "fact" => app.type_filter = app::TypeFilter::Fact,
                "proc" | "procedure" => app.type_filter = app::TypeFilter::Procedure,
                "pref" | "preference" => app.type_filter = app::TypeFilter::Preference,
                "dec" | "decision" => app.type_filter = app::TypeFilter::Decision,
                "obs" | "observation" => app.type_filter = app::TypeFilter::Observation,
                _ => app.type_filter = app::TypeFilter::All,
            }
            app.set_status(format!("Set type filter to [{}]", app.type_filter));
            app.mode = Mode::Browse;
            data::spawn_list_paged_fetch(
                store.clone(),
                fetch_tx.clone(),
                app.type_filter.to_memory_type(),
                app.page,
                app.page_size,
                app.show_archived,
            );
        }
    } else if lower.starts_with(":sort ") {
        let parts: Vec<_> = cmd.split_whitespace().collect();
        if parts.len() > 1 {
            match parts[1].to_lowercase().as_str() {
                "decay" => app.sort_field = app::SortField::Decay,
                "created" | "created_at" => app.sort_field = app::SortField::CreatedAt,
                _ => app.sort_field = app::SortField::Importance,
            }
            app.sort_memories();
            app.set_status(format!("Set sort field to [{}]", app.sort_field));
            app.mode = Mode::Browse;
        }
    } else if lower.starts_with(":search ") || lower.starts_with(":s ") {
        let query = cmd.split_once(' ').map(|x| x.1).unwrap_or("").to_string();
        app.filter_input = query.clone();
        app.set_status(format!("Searching FTS for '{}'...", query));
        app.mode = Mode::Browse;
        data::spawn_search_fetch(store.clone(), fetch_tx.clone(), query, 100);
    } else {
        // Default / :recall query
        let query = if let Some(stripped) = cmd.strip_prefix(":recall ") {
            stripped.trim().to_string()
        } else {
            cmd.trim().to_string()
        };
        app.set_status(format!("Recalling LLM reasoning for '{}'...", query));
        app.loading = true;
        data::spawn_recall_fetch(engine.clone(), fetch_tx.clone(), query, 10);
    }
}
