//! Guided LLM Recall Results View rendering.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, Mode};

/// Render the Guided LLM Recall Results view pane.
pub fn draw_recall_results(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == Mode::RecallResults;
    let border_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let count = app.recall_results.len();
    let query_str = if app.recall_input.is_empty() {
        "active query"
    } else {
        &app.recall_input
    };

    // --- Results List ---
    let items: Vec<ListItem> = app
        .recall_results
        .iter()
        .enumerate()
        .map(|(idx, res)| {
            let is_selected = idx == app.recall_selected;
            let sim_pct = (res.similarity * 100.0) as u32;

            let score_style = if sim_pct >= 80 {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else if sim_pct >= 50 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let prefix = if is_selected { "▶ " } else { "  " };
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(format!("[{:>2}% match] ", sim_pct), score_style),
                Span::styled(
                    format!("[{:^10}] ", res.memory_type.to_string()),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    res.content.clone(),
                    if is_selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
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
        " 🧠 Guided LLM Recall Results for '{}' ({} items) — ↑/↓ navigate, Enter inspect, Esc back ",
        query_str, count
    );

    let list_widget = List::new(items).block(
        Block::default()
            .borders(Borders::TOP)
            .title(list_title)
            .border_style(border_style),
    );
    f.render_widget(list_widget, chunks[0]);

    // --- Reasoning Trace & Selected Details ---
    let detail_text = if let Some(selected_res) = app.selected_recall_result() {
        let reasoning = selected_res
            .reasoning
            .as_deref()
            .unwrap_or("No LLM reasoning trace generated for this memory candidate.");

        vec![
            Line::from(vec![
                Span::styled("Memory ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    selected_res.id.to_string(),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Similarity Score: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "{:.4} ({}%)",
                        selected_res.similarity,
                        (selected_res.similarity * 100.0) as u32
                    ),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("  Decay Score: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.2}", selected_res.decay_score),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Content:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                &selected_res.content,
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "LLM Guided Reasoning Trace:",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                reasoning,
                Style::default().fg(Color::LightMagenta),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No recall result selected.",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let detail_widget = Paragraph::new(detail_text)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .title(" Selected Recall Reasoning & Trace ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(detail_widget, chunks[1]);
}
