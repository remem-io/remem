//! Session Transcript & Consolidation Viewer widget.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;

/// Render the Session Viewer.
pub fn draw_session_viewer(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let count = app.session_summaries.len();

    // --- Sessions List ---
    let items: Vec<ListItem> = app
        .session_summaries
        .iter()
        .enumerate()
        .map(|(idx, sess)| {
            let is_selected = idx == app.session_selected;
            let prefix = if is_selected { "▶ " } else { "  " };

            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("[{}] ", sess.session_id),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("project: {} ", sess.project),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("({})", sess.timestamp.format("%Y-%m-%d %H:%M:%S")),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let style = if is_selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list_title = format!(
        " 📄 Session Summaries ({} records) — ↑/↓ navigate, c trigger consolidation, Esc back ",
        count
    );

    let list_widget = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(list_title)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(list_widget, chunks[0]);

    // --- Session Summary Inspector ---
    let detail_text = if let Some(sess) = app.session_summaries.get(app.session_selected) {
        let files_str = if sess.files_touched.is_empty() {
            "—".to_string()
        } else {
            sess.files_touched.join(", ")
        };

        let decisions_str = if sess.key_decisions.is_empty() {
            "—".to_string()
        } else {
            sess.key_decisions.join("; ")
        };

        vec![
            Line::from(vec![
                Span::styled("Session ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &sess.session_id,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Project: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&sess.project, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Files Touched: ", Style::default().fg(Color::DarkGray)),
                Span::styled(files_str, Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("Key Decisions: ", Style::default().fg(Color::DarkGray)),
                Span::styled(decisions_str, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Session Summary Narrative:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                &sess.summary,
                Style::default().fg(Color::White),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No session summary record selected.",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let detail_widget = Paragraph::new(detail_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Session Narrative & Inspection ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(detail_widget, chunks[1]);
}
