//! Memory browser — table view of all memory records with multi-selection support.

use chrono::Utc;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use crate::tui::app::{App, Mode};

/// Render the memory browser table into the given area.
pub fn draw_browser(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = matches!(app.mode, Mode::Browse | Mode::SearchInput);
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let total_pages = if app.total_count == 0 {
        1
    } else {
        app.total_count.div_ceil(app.page_size)
    };

    let sel_indicator = if app.selected_set.is_empty() {
        "".to_string()
    } else {
        format!(" ({} sel)", app.selected_set.len())
    };

    let sort_indicator = format!(
        " Page {}/{} (Total {}{}) | filter: {} | sort: {} {} ",
        app.page + 1,
        total_pages,
        app.total_count,
        sel_indicator,
        app.type_filter,
        app.sort_field,
        if app.sort_ascending { "↑" } else { "↓" }
    );

    let title = if app.loading {
        format!(" Memories [loading…] — {} ", sort_indicator)
    } else {
        format!(" Memories — {} ", sort_indicator)
    };

    let table_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);

    if app.memories.is_empty() {
        let empty_lines = if app.loading {
            vec![Line::from(""), Line::from("Loading memories...")]
        } else if !app.filter_input.trim().is_empty() {
            vec![
                Line::from(""),
                Line::from("No memories match the current search."),
                Line::from("Clear the search or adjust the query."),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from("No memories found."),
                Line::from("Press n to create a memory or r to refresh."),
            ]
        };

        let empty = Paragraph::new(empty_lines)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(table_block);
        f.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("Sel").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Type").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Content").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Imp").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Decay").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Tags").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Age").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().fg(Color::Yellow))
    .height(1);

    let now = Utc::now();
    let rows: Vec<Row> = app
        .memories
        .iter()
        .map(|m| {
            let is_selected = app.selected_set.contains(&m.id);
            let sel_mark = if is_selected { "[x]" } else { "[ ]" };
            let sel_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let type_color = match m.memory_type {
                rememhq_core::memory::types::MemoryType::Fact => Color::Green,
                rememhq_core::memory::types::MemoryType::Procedure => Color::Blue,
                rememhq_core::memory::types::MemoryType::Preference => Color::Magenta,
                rememhq_core::memory::types::MemoryType::Decision => Color::Yellow,
                rememhq_core::memory::types::MemoryType::Observation => Color::Cyan,
            };

            let content_preview: String = m
                .content
                .chars()
                .take(60)
                .collect::<String>()
                .replace('\n', " ");

            let age = format_relative_time(now, m.created_at);
            let tags_str = if m.tags.is_empty() {
                "—".to_string()
            } else {
                m.tags.join(", ")
            };

            let importance_color = if m.importance >= 8.0 {
                Color::Red
            } else if m.importance >= 5.0 {
                Color::Yellow
            } else {
                Color::White
            };

            let decay_color = if m.decay_score < 0.3 {
                Color::Red
            } else if m.decay_score < 0.7 {
                Color::Yellow
            } else {
                Color::Green
            };

            Row::new(vec![
                Cell::from(sel_mark).style(sel_style),
                Cell::from(m.memory_type.to_string()).style(Style::default().fg(type_color)),
                Cell::from(content_preview),
                Cell::from(format!("{:.1}", m.importance))
                    .style(Style::default().fg(importance_color)),
                Cell::from(format!("{:.2}", m.decay_score)).style(Style::default().fg(decay_color)),
                Cell::from(tags_str).style(Style::default().fg(Color::DarkGray)),
                Cell::from(age).style(Style::default().fg(Color::DarkGray)),
            ])
            .height(1)
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(5),
        ratatui::layout::Constraint::Length(12),
        ratatui::layout::Constraint::Min(20),
        ratatui::layout::Constraint::Length(5),
        ratatui::layout::Constraint::Length(6),
        ratatui::layout::Constraint::Length(18),
        ratatui::layout::Constraint::Length(8),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(table_block)
        .row_highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Cyan),
        )
        .highlight_symbol("▶ ");

    let mut state = TableState::default();
    if !app.memories.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(table, area, &mut state);
}

/// Render the search/filter input bar.
pub fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let is_search = app.mode == Mode::SearchInput;
    let style = if is_search {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let label = if is_search { " Search: " } else { " / search " };
    let input_text = if is_search || !app.filter_input.is_empty() {
        app.filter_input.as_str()
    } else {
        ""
    };
    let placeholder = if input_text.is_empty() {
        Some(if is_search {
            "type to search memory content"
        } else {
            "press / to search, : for commands"
        })
    } else {
        None
    };

    let mut spans = vec![Span::styled(label, style.add_modifier(Modifier::BOLD))];
    if let Some(text) = placeholder {
        spans.push(Span::styled(text, Style::default().fg(Color::DarkGray)));
    } else {
        spans.push(Span::styled(input_text, Style::default().fg(Color::White)));
    }

    let line = Line::from(spans);

    let block = Block::default().borders(Borders::ALL).border_style(style);

    let paragraph = ratatui::widgets::Paragraph::new(line).block(block);
    f.render_widget(paragraph, area);

    // Show cursor when in search mode.
    if is_search && area.width > 2 && area.height > 1 {
        let content_width = area.width.saturating_sub(2);
        let cursor_offset = (label.len() + app.filter_cursor) as u16;
        let cursor_x = area.x + 1 + cursor_offset.min(content_width.saturating_sub(1));
        let cursor_y = area.y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Format a timestamp as a relative "time ago" string.
fn format_relative_time(now: chrono::DateTime<Utc>, then: chrono::DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(then);
    let seconds = duration.num_seconds();
    if seconds < 60 {
        "now".to_string()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}
