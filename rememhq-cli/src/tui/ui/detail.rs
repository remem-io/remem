//! Detail pane — shows full information about the selected memory record.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::{App, Mode};

/// Render the detail pane for the currently selected memory.
pub fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == Mode::Detail;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let Some(memory) = app.selected_memory() else {
        let empty = Paragraph::new(" No memory selected")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Detail ")
                    .border_style(border_style),
            );
        f.render_widget(empty, area);
        return;
    };

    let label_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(Color::White);
    let dim_style = Style::default().fg(Color::DarkGray);

    let mut lines: Vec<Line> = Vec::new();

    // ID
    lines.push(Line::from(vec![
        Span::styled("ID:          ", label_style),
        Span::styled(memory.id.to_string(), dim_style),
    ]));

    // Type
    let type_color = match memory.memory_type {
        rememhq_core::memory::types::MemoryType::Fact => Color::Green,
        rememhq_core::memory::types::MemoryType::Procedure => Color::Blue,
        rememhq_core::memory::types::MemoryType::Preference => Color::Magenta,
        rememhq_core::memory::types::MemoryType::Decision => Color::Yellow,
        rememhq_core::memory::types::MemoryType::Observation => Color::Cyan,
    };
    lines.push(Line::from(vec![
        Span::styled("Type:        ", label_style),
        Span::styled(
            memory.memory_type.to_string(),
            Style::default().fg(type_color).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Observation kind
    if let Some(ref kind) = memory.observation_kind {
        lines.push(Line::from(vec![
            Span::styled("Obs Kind:    ", label_style),
            Span::styled(kind.to_string(), value_style),
        ]));
    }

    // Importance & Decay
    lines.push(Line::from(vec![
        Span::styled("Importance:  ", label_style),
        Span::styled(format!("{:.1} / 10.0", memory.importance), value_style),
        Span::styled("    Decay: ", label_style),
        Span::styled(format!("{:.3}", memory.decay_score), value_style),
    ]));

    // Tags
    let tags_str = if memory.tags.is_empty() {
        "—".to_string()
    } else {
        memory.tags.join(", ")
    };
    lines.push(Line::from(vec![
        Span::styled("Tags:        ", label_style),
        Span::styled(tags_str, Style::default().fg(Color::Cyan)),
    ]));

    // Timestamps
    lines.push(Line::from(vec![
        Span::styled("Created:     ", label_style),
        Span::styled(
            memory
                .created_at
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string(),
            dim_style,
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Updated:     ", label_style),
        Span::styled(
            memory
                .updated_at
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string(),
            dim_style,
        ),
    ]));

    // Source session
    if let Some(ref session) = memory.source_session {
        lines.push(Line::from(vec![
            Span::styled("Session:     ", label_style),
            Span::styled(session.clone(), dim_style),
        ]));
    }

    // Store/path
    if let Some(ref store_id) = memory.store_id {
        let path_str = memory.path.as_deref().unwrap_or("—");
        lines.push(Line::from(vec![
            Span::styled("Store:       ", label_style),
            Span::styled(format!("{} / {}", store_id, path_str), dim_style),
        ]));
    }

    // TTL
    if let Some(ttl) = memory.ttl_days {
        lines.push(Line::from(vec![
            Span::styled("TTL:         ", label_style),
            Span::styled(format!("{} days", ttl), value_style),
        ]));
    }

    // Separator
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "─── Content ───────────────────────────────────────",
        dim_style,
    )));
    lines.push(Line::from(""));

    // Content (plain text for MVP — see guide §7b).
    for content_line in memory.content.lines() {
        lines.push(Line::from(Span::styled(
            content_line.to_string(),
            value_style,
        )));
    }

    let title = format!(" Detail — {} ", &memory.id.to_string()[..8]);
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));

    f.render_widget(paragraph, area);
}
