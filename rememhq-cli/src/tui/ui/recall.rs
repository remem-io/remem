//! Universal Command Palette & Guided LLM Recall Query input bar.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::{App, Mode};

/// Render the Universal Command Palette input bar.
pub fn draw_recall_bar(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.mode == Mode::RecallInput;
    let style = if is_active {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let label = " : ";
    let input_text = app.recall_input.as_str();

    let input_span = if input_text.is_empty() {
        Span::styled(
            "recall query or command",
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::styled(input_text, Style::default().fg(Color::White))
    };

    let line = Line::from(vec![
        Span::styled(label, style.add_modifier(Modifier::BOLD)),
        input_span,
    ]);

    let title_str = if app.command_history.is_empty() {
        " Universal Command Palette (:recall <query>, :search, :filter, :sort, :clear, :q) "
    } else {
        " Universal Command Palette (↑/↓ command history) "
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .title(title_str)
        .border_style(style);

    let paragraph = Paragraph::new(line).block(block);
    f.render_widget(paragraph, area);

    if is_active && area.width > 2 && area.height > 1 {
        let content_width = area.width.saturating_sub(2);
        let cursor_offset = (label.len() + app.recall_cursor) as u16;
        let cursor_x = area.x + 1 + cursor_offset.min(content_width.saturating_sub(1));
        let cursor_y = area.y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}
