//! Memory Version History & Diff Viewer widget.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;

/// Render the Memory Version History & Diff view.
pub fn draw_version_history(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let mem_id_str = app
        .selected_memory()
        .map(|m| m.id.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let count = app.version_history.len();

    // --- Version List ---
    let items: Vec<ListItem> = app
        .version_history
        .iter()
        .enumerate()
        .map(|(idx, ver)| {
            let is_selected = idx == app.version_selected;
            let prefix = if is_selected { "▶ " } else { "  " };

            let op_color = match ver.operation.as_str() {
                "insert" => Color::Green,
                "update" => Color::Yellow,
                "archive" => Color::Red,
                "decay" => Color::Magenta,
                _ => Color::Cyan,
            };

            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("[{:<8}] ", ver.operation),
                    Style::default().fg(op_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("ver: {} ", &ver.id[..std::cmp::min(8, ver.id.len())]),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("({}) ", ver.created_at.format("%Y-%m-%d %H:%M:%S")),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!(
                        "sha: {}",
                        &ver.content_sha256[..std::cmp::min(7, ver.content_sha256.len())]
                    ),
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
        " 📜 Memory Version History for {} ({} revisions) — ↑/↓ navigate, Esc back ",
        &mem_id_str[..std::cmp::min(8, mem_id_str.len())],
        count
    );

    let list_widget = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(list_title)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(list_widget, chunks[0]);

    // --- Content & Snapshot View ---
    let detail_text = if let Some(ver) = app.version_history.get(app.version_selected) {
        vec![
            Line::from(vec![
                Span::styled("Version ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&ver.id, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Operation: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&ver.operation, Style::default().fg(Color::Yellow)),
                Span::styled("  Timestamp: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    ver.created_at.to_rfc3339(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("SHA256: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&ver.content_sha256, Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Historical Content Snapshot:",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                &ver.content,
                Style::default().fg(Color::White),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No version history record selected.",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let detail_widget = Paragraph::new(detail_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Revision Snapshot Details ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(detail_widget, chunks[1]);
}
