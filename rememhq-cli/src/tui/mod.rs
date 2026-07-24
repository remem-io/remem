//! `remem tui` — Terminal UI for browsing and inspecting memory.

pub mod app;
pub mod data;
pub mod event;
pub mod ui;

use std::io;
use std::panic;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{Event, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use rememhq_core::config::RememConfig;
use rememhq_core::reasoning::ReasoningEngine;
use tokio::sync::mpsc;

use app::{App, Mode};
use event::{AppEvent, FetchResult};

/// Entry point for `remem tui`.
pub async fn run_tui(engine: ReasoningEngine, _config: &RememConfig) -> anyhow::Result<()> {
    // Install panic hook to restore terminal state before default panic handler prints.
    install_panic_hook();

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_tui_loop(&mut terminal, engine).await;

    // Terminal teardown
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

/// Main async event loop for the TUI.
async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    engine: ReasoningEngine,
) -> anyhow::Result<()> {
    let mut app = App::new();

    let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<FetchResult>();
    let store = engine.store.clone();
    let engine_arc = Arc::new(engine);

    // Initial data fetch
    data::spawn_list_fetch(
        store.clone(),
        fetch_tx.clone(),
        app.type_filter.to_memory_type(),
        100,
    );
    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());

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
                Mode::Help => match key.code {
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('q') => {
                        app.mode = app.previous_mode;
                    }
                    _ => {}
                },

                Mode::ConfirmArchive => match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if let Some(id) = app.archive_target {
                            app.set_status(format!("Archiving memory {}...", &id.to_string()[..8]));
                            data::spawn_archive_task(store.clone(), fetch_tx.clone(), id);
                        }
                        app.mode = Mode::Browse;
                        app.archive_target = None;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        app.mode = Mode::Browse;
                        app.archive_target = None;
                    }
                    _ => {}
                },

                Mode::Search => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Browse;
                        app.filter_input.clear();
                        app.filter_cursor = 0;
                        app.loading = true;
                        data::spawn_list_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            app.type_filter.to_memory_type(),
                            100,
                        );
                    }
                    KeyCode::Enter => {
                        app.mode = Mode::Browse;
                        app.loading = true;
                        if app.filter_input.trim().is_empty() {
                            data::spawn_list_fetch(
                                store.clone(),
                                fetch_tx.clone(),
                                app.type_filter.to_memory_type(),
                                100,
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
                    KeyCode::Backspace => app.filter_backspace(),
                    KeyCode::Left => app.filter_cursor_left(),
                    KeyCode::Right => app.filter_cursor_right(),
                    KeyCode::Char(c) => app.filter_insert_char(c),
                    _ => {}
                },

                Mode::RecallQuery => match key.code {
                    KeyCode::Esc => {
                        app.mode = Mode::Browse;
                        app.recall_input.clear();
                        app.recall_cursor = 0;
                    }
                    KeyCode::Enter => {
                        let query = app.recall_input.trim().to_string();
                        if !query.is_empty() {
                            app.set_status(format!("Recalling LLM reasoning for '{}'...", query));
                            app.loading = true;
                            data::spawn_recall_fetch(engine.clone(), fetch_tx.clone(), query, 10);
                        }
                        app.mode = Mode::Browse;
                    }
                    KeyCode::Backspace => app.recall_backspace(),
                    KeyCode::Left => app.recall_cursor_left(),
                    KeyCode::Right => app.recall_cursor_right(),
                    KeyCode::Char(c) => app.recall_insert_char(c),
                    _ => {}
                },

                Mode::Detail => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Browse,
                    KeyCode::Down | KeyCode::Char('j') => app.detail_scroll += 1,
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.detail_scroll = app.detail_scroll.saturating_sub(1);
                    }
                    KeyCode::PageDown => app.detail_scroll += 10,
                    KeyCode::PageUp => {
                        app.detail_scroll = app.detail_scroll.saturating_sub(10);
                    }
                    KeyCode::Home => app.detail_scroll = 0,
                    KeyCode::Char('d') => {
                        if let Some(mem) = app.selected_memory() {
                            app.archive_target = Some(mem.id);
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmArchive;
                        }
                    }
                    _ => {}
                },

                Mode::Browse => match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                    KeyCode::PageDown => app.page_down(),
                    KeyCode::PageUp => app.page_up(),
                    KeyCode::Home => app.select_first(),
                    KeyCode::End => app.select_last(),
                    KeyCode::Enter => {
                        if !app.memories.is_empty() {
                            app.mode = Mode::Detail;
                        }
                    }
                    KeyCode::Char('/') => {
                        app.mode = Mode::Search;
                    }
                    KeyCode::Char(':') => {
                        app.mode = Mode::RecallQuery;
                    }
                    KeyCode::Char('t') => {
                        app.type_filter = app.type_filter.next();
                        app.loading = true;
                        data::spawn_list_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            app.type_filter.to_memory_type(),
                            100,
                        );
                    }
                    KeyCode::Char('s') => {
                        app.sort_field = app.sort_field.next();
                        app.sort_memories();
                    }
                    KeyCode::Char('S') => {
                        app.sort_ascending = !app.sort_ascending;
                        app.sort_memories();
                    }
                    KeyCode::Char('d') => {
                        if let Some(mem) = app.selected_memory() {
                            app.archive_target = Some(mem.id);
                            app.previous_mode = app.mode;
                            app.mode = Mode::ConfirmArchive;
                        }
                    }
                    KeyCode::Char('r') => {
                        app.loading = true;
                        app.set_status("Refreshed store records & stats");
                        data::spawn_list_fetch(
                            store.clone(),
                            fetch_tx.clone(),
                            app.type_filter.to_memory_type(),
                            100,
                        );
                        data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                    }
                    KeyCode::Char('?') | KeyCode::Char('h') => {
                        app.previous_mode = app.mode;
                        app.mode = Mode::Help;
                    }
                    KeyCode::Tab => {
                        app.mode = Mode::Stats;
                    }
                    KeyCode::Char('m') => {
                        app.mode = Mode::Monitor;
                    }
                    _ => {}
                },

                Mode::Stats => match key.code {
                    KeyCode::Tab => app.mode = Mode::Monitor,
                    KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Browse,
                    KeyCode::Char('?') | KeyCode::Char('h') => {
                        app.previous_mode = app.mode;
                        app.mode = Mode::Help;
                    }
                    _ => {}
                },

                Mode::Monitor => match key.code {
                    KeyCode::Tab => app.mode = Mode::Browse,
                    KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Browse,
                    KeyCode::Char('?') | KeyCode::Char('h') => {
                        app.previous_mode = app.mode;
                        app.mode = Mode::Help;
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
            // Periodically refresh stats
            data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
        }

        AppEvent::FetchComplete(result) => match result {
            FetchResult::Memories(res) => {
                app.loading = false;
                if let Ok(memories) = res {
                    app.memories = memories;
                    app.sort_memories();
                }
            }
            FetchResult::Recall(res) => {
                app.loading = false;
                if let Ok(results) = res {
                    app.set_status(format!("Recall retrieved {} results", results.len()));
                    app.recall_results = results;
                }
            }
            FetchResult::Stats(res) => {
                if let Ok(stats) = res {
                    app.stats = Some(stats);
                }
            }
            FetchResult::Archived(id, res) => {
                if matches!(res, Ok(true)) {
                    app.set_status(format!(
                        "Successfully archived memory {}",
                        &id.to_string()[..8]
                    ));
                    // Re-fetch active memory list
                    data::spawn_list_fetch(
                        store.clone(),
                        fetch_tx.clone(),
                        app.type_filter.to_memory_type(),
                        100,
                    );
                    data::spawn_stats_fetch(store.clone(), fetch_tx.clone());
                } else {
                    app.set_status("Failed to archive memory");
                }
            }
        },
    }
}

/// Install a panic hook that resets terminal mode before printing the panic.
fn install_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}
