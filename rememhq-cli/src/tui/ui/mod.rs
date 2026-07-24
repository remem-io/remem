//! UI rendering module — top-level layout split and pane composition.

pub mod browser;
pub mod confirm;
pub mod detail;
pub mod help;
pub mod monitor;
pub mod recall;
pub mod stats;

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
        Mode::Help => {
            help::draw_help_modal(f, size);
        }
        Mode::ConfirmArchive => {
            confirm::draw_confirm_modal(f, app, size);
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

    if app.mode == Mode::RecallQuery {
        recall::draw_recall_bar(f, app, left_chunks[1]);
    } else {
        browser::draw_search_bar(f, app, left_chunks[1]);
    }

    // Right column content depends on mode
    match app.mode {
        Mode::Monitor => {
            monitor::draw_monitor(f, app, cols[1]);
        }
        Mode::Stats => {
            stats::draw_stats(f, app, cols[1]);
        }
        _ => {
            // Split right column into Detail pane (top 60%) and Stats (bottom 40%)
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(60), // Detail pane
                    Constraint::Percentage(40), // Stats dashboard
                ])
                .split(cols[1]);

            detail::draw_detail(f, app, right_chunks[0]);
            stats::draw_stats(f, app, right_chunks[1]);
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
        Mode::Search => vec![
            Span::styled("Esc", key_style),
            Span::styled(": cancel  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": run search  ", desc_style),
        ],
        Mode::RecallQuery => vec![
            Span::styled("Esc", key_style),
            Span::styled(": cancel  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": run guided recall", desc_style),
        ],
        Mode::ConfirmArchive => vec![
            Span::styled("y", key_style),
            Span::styled(": confirm archive  ", desc_style),
            Span::styled("n / Esc", key_style),
            Span::styled(": cancel", desc_style),
        ],
        Mode::Help => vec![
            Span::styled("Esc / ? / q", key_style),
            Span::styled(": close help window", desc_style),
        ],
        Mode::Detail => vec![
            Span::styled("↑/↓/j/k", key_style),
            Span::styled(": scroll  ", desc_style),
            Span::styled("d", key_style),
            Span::styled(": archive  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(": back to browser  ", desc_style),
            Span::styled("q", key_style),
            Span::styled(": quit", desc_style),
        ],
        _ => vec![
            Span::styled("↑/↓/j/k", key_style),
            Span::styled(": navigate  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(": detail  ", desc_style),
            Span::styled("/", key_style),
            Span::styled(": search  ", desc_style),
            Span::styled("t", key_style),
            Span::styled(": filter type  ", desc_style),
            Span::styled("s", key_style),
            Span::styled(": sort  ", desc_style),
            Span::styled("d", key_style),
            Span::styled(": archive  ", desc_style),
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
    use crate::tui::app::{App, Mode};
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
        app.mode = Mode::Help;

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }

    #[test]
    fn test_draw_confirm_archive_modal() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.mode = Mode::ConfirmArchive;
        app.archive_target = Some(uuid::Uuid::new_v4());

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert!(!buffer.content().is_empty());
    }
}
