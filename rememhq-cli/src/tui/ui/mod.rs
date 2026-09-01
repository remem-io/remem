//! UI rendering module — top-level layout split and pane composition.

pub mod browser;
pub mod confirm;
pub mod context_usage;
pub mod detail;
pub mod edit;
pub mod graph;
pub mod help;
pub mod monitor;
pub mod new_memory;
pub mod recall;
pub mod recall_results;
pub mod sessions;
pub mod stats;
pub mod telemetry_dashboard;
pub mod versions;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, Mode};

/// Draw the entire UI layout and modal overlays.
pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();

    // Vertical layout: Header (1) -> Main Content (Min 10) -> Help Footer (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header bar
            Constraint::Min(10),   // Main area
            Constraint::Length(1), // Help footer bar
        ])
        .split(size);

    draw_header(f, app, chunks[0]);
    draw_main_area(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    // Modal overlays rendered over top of the layout
    match app.mode {
        Mode::HelpModal => {
            help::draw_help_modal(f, app, size);
        }
        Mode::ConfirmModal => {
            confirm::draw_confirm_modal(f, app, size);
        }
        Mode::EditModal => {
            edit::draw_edit_modal(f, app, size);
        }
        Mode::NewMemoryModal => {
            new_memory::draw_new_memory_modal(f, app, size);
        }
        _ => {}
    }
}

/// Render top status/title bar.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mode_str = format!("{:?}", app.mode);
    let status_str = app.active_status().unwrap_or("");

    let mut header_spans = vec![
        Span::styled(
            " remem tui ",
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Mode: {}", mode_str),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Filter: [{}]", app.type_filter),
            Style::default().fg(Color::Green),
        ),
    ];

    if !status_str.is_empty() {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(
            format!("★ {}", status_str),
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let paragraph = Paragraph::new(Line::from(header_spans));
    f.render_widget(paragraph, area);
}

/// Render the main area (split into browser column and detail/stats column).
fn draw_main_area(f: &mut Frame, app: &App, area: Rect) {
    if app.mode == Mode::RecallResults {
        recall_results::draw_recall_results(f, app, area);
        return;
    }

    if app.mode == Mode::VersionHistory {
        versions::draw_version_history(f, app, area);
        return;
    }

    if app.mode == Mode::GraphBrowser {
        graph::draw_graph_browser(f, app, area);
        return;
    }

    if app.mode == Mode::SessionViewer {
        sessions::draw_session_viewer(f, app, area);
        return;
    }

    if app.mode == Mode::Telemetry {
        telemetry_dashboard::draw_telemetry_dashboard(f, app, area);
        return;
    }

    if area.width < 110 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(58),
                Constraint::Length(3),
                Constraint::Percentage(42),
            ])
            .split(area);

        browser::draw_browser(f, app, rows[0]);

        if app.mode == Mode::RecallInput {
            recall::draw_recall_bar(f, app, rows[1]);
        } else {
            browser::draw_search_bar(f, app, rows[1]);
        }

        draw_secondary_area(f, app, rows[2], false);
        return;
    }

    // Horizontal split: Left column (Browser + Input bar) | Right column (Detail / Stats / Monitor)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // Browser column
            Constraint::Percentage(45), // Detail/Stats/Monitor column
        ])
        .split(area);

    // Left column: Browser table (top) + Input bar (bottom)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Table
            Constraint::Length(3), // Search / Recall bar
        ])
        .split(cols[0]);

    browser::draw_browser(f, app, left_chunks[0]);

    if app.mode == Mode::RecallInput {
        recall::draw_recall_bar(f, app, left_chunks[1]);
    } else {
        browser::draw_search_bar(f, app, left_chunks[1]);
    }

    draw_secondary_area(f, app, cols[1], true);
}

/// Render the secondary pane area to keep wide and narrow layouts consistent.
fn draw_secondary_area(f: &mut Frame, app: &App, area: Rect, include_overview_stats: bool) {
    match app.mode {
        Mode::Monitor => {
            monitor::draw_monitor(f, app, area);
        }
        Mode::Stats => {
            if area.height >= 16 {
                let stats_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(area);
                stats::draw_stats(f, app, stats_chunks[0]);
                context_usage::draw_context_usage(f, app, stats_chunks[1]);
            } else {
                stats::draw_stats(f, app, area);
            }
        }
        _ => {
            if include_overview_stats && area.height >= 18 {
                let right_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(62), // Detail pane
                        Constraint::Percentage(38), // Stats dashboard
                    ])
                    .split(area);

                detail::draw_detail(f, app, right_chunks[0]);
                stats::draw_stats(f, app, right_chunks[1]);
            } else {
                detail::draw_detail(f, app, area);
            }
        }
    }
}

/// Render footer with keybinding instructions.
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::DarkGray);

    let hints = match app.mode {
        Mode::SearchInput => vec![
            Span::styled("Esc", key_style),
            Span::styled(": cancel  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": run search  ", desc_style),
        ],
        Mode::RecallInput => vec![
            Span::styled("Esc", key_style),
            Span::styled(": cancel  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": run guided recall", desc_style),
        ],
        Mode::RecallResults => vec![
            Span::styled("↑/↓", key_style),
            Span::styled(": navigate results  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": inspect in detail  ", desc_style),
            Span::styled("Esc / b", key_style),
            Span::styled(": back to browser", desc_style),
        ],
        Mode::GraphBrowser => vec![
            Span::styled("↑/↓", key_style),
            Span::styled(": navigate triples  ", desc_style),
            Span::styled("Esc / b", key_style),
            Span::styled(": back to browser", desc_style),
        ],
        Mode::SessionViewer => vec![
            Span::styled("↑/↓", key_style),
            Span::styled(": navigate sessions  ", desc_style),
            Span::styled("c", key_style),
            Span::styled(": consolidate  ", desc_style),
            Span::styled("Esc / b", key_style),
            Span::styled(": back to browser", desc_style),
        ],
        Mode::VersionHistory => vec![
            Span::styled("↑/↓", key_style),
            Span::styled(": navigate history  ", desc_style),
            Span::styled("Esc / b", key_style),
            Span::styled(": back to browser", desc_style),
        ],
        Mode::Telemetry => vec![
            Span::styled("r", key_style),
            Span::styled(": refresh metrics  ", desc_style),
            Span::styled("Esc / b", key_style),
            Span::styled(": back to browser  ", desc_style),
            Span::styled("q", key_style),
            Span::styled(": quit", desc_style),
        ],
        Mode::EditModal => vec![
            Span::styled("Tab", key_style),
            Span::styled(": next field  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": save memory record  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(": cancel edit", desc_style),
        ],
        Mode::NewMemoryModal => vec![
            Span::styled("Tab", key_style),
            Span::styled(": next field  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": create memory  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(": cancel", desc_style),
        ],
        Mode::ConfirmModal => vec![
            Span::styled("y", key_style),
            Span::styled(": confirm action  ", desc_style),
            Span::styled("n / Esc", key_style),
            Span::styled(": cancel", desc_style),
        ],
        Mode::HelpModal => vec![
            Span::styled("Esc / ? / q", key_style),
            Span::styled(": close help window", desc_style),
        ],
        Mode::Detail => vec![
            Span::styled("↑/↓/j/k", key_style),
            Span::styled(": scroll  ", desc_style),
            Span::styled("e", key_style),
            Span::styled(": edit  ", desc_style),
            Span::styled("v", key_style),
            Span::styled(": history  ", desc_style),
            Span::styled("d", key_style),
            Span::styled(": archive  ", desc_style),
            Span::styled("u", key_style),
            Span::styled(": unarchive  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(": back to browser  ", desc_style),
            Span::styled("q", key_style),
            Span::styled(": quit", desc_style),
        ],
        Mode::Monitor => vec![
            Span::styled("↑/↓", key_style),
            Span::styled(": scroll  ", desc_style),
            Span::styled("p", key_style),
            Span::styled(": pause/resume  ", desc_style),
            Span::styled("G", key_style),
            Span::styled(": jump to latest  ", desc_style),
            Span::styled("PgUp/PgDn", key_style),
            Span::styled(": fast scroll  ", desc_style),
            Span::styled("Esc/b", key_style),
            Span::styled(": back  ", desc_style),
            Span::styled("q", key_style),
            Span::styled(": quit", desc_style),
        ],
        _ => vec![
            Span::styled("Tab", key_style),
            Span::styled(": cycle pane  ", desc_style),
            Span::styled("g/S", key_style),
            Span::styled(": graph/sessions  ", desc_style),
            Span::styled("Space", key_style),
            Span::styled(": sel  ", desc_style),
            Span::styled("Shift+A/D/C", key_style),
            Span::styled(": bulk archive/del/decay  ", desc_style),
            Span::styled("?", key_style),
            Span::styled(": help  ", desc_style),
            Span::styled("q", key_style),
            Span::styled(": quit", desc_style),
        ],
    };

    let footer = Paragraph::new(Line::from(hints));
    f.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, ConfirmAction, Mode};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use rememhq_core::memory::types::{MemoryRecord, MemoryType};

    #[test]
    fn test_draw_browse_mode() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.loading = false;
        app.memories
            .push(MemoryRecord::new("Test memory content", MemoryType::Fact));

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_draw_stats_mode() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.mode = Mode::Stats;

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_draw_help_modal() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.mode = Mode::HelpModal;

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_draw_confirm_archive_modal() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.mode = Mode::ConfirmModal;
        app.confirm_action = Some(ConfirmAction::Archive(uuid::Uuid::new_v4()));

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_draw_narrow_browse_layout() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.memories.push(MemoryRecord::new(
            "Narrow terminal memory",
            MemoryType::Fact,
        ));

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_draw_telemetry_mode() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.mode = Mode::Telemetry;

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }
}
