//! Knowledge Graph & Entity Browser widget.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;

/// Render the Knowledge Graph & Entity Browser view.
pub fn draw_graph_browser(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let count = app.graph_triples.len();

    // --- Triples List ---
    let items: Vec<ListItem> = app
        .graph_triples
        .iter()
        .enumerate()
        .map(|(idx, triple)| {
            let is_selected = idx == app.graph_selected;
            let prefix = if is_selected { "▶ " } else { "  " };

            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("({}) ", triple.subject),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("--[{}]--> ", triple.predicate),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    format!("({})", triple.object),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
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
        " 🌐 Knowledge Graph Triples ({} relationships) — ↑/↓ navigate, Esc back ",
        count
    );

    let list_widget = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(list_title)
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(list_widget, chunks[0]);

    // --- Entity Relationship Inspector ---
    let detail_text = if let Some(triple) = app.graph_triples.get(app.graph_selected) {
        vec![
            Line::from(vec![
                Span::styled("Subject Entity: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &triple.subject,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "Relationship / Predicate: ",
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    &triple.predicate,
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Object Entity: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &triple.object,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Graph Representation:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(
                    "  ({}) ==== [{}] ====> ({})",
                    triple.subject, triple.predicate, triple.object
                ),
                Style::default().fg(Color::White),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No knowledge triple selected.",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let detail_widget = Paragraph::new(detail_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Relationship Inspector ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(detail_widget, chunks[1]);
}
