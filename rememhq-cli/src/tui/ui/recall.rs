//! Guided LLM Recall Query input & result rendering.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::{App, Mode};

/// Render the Guided LLM Recall query input bar.
pub fn draw_recall_bar(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.mode == Mode::RecallQuery;
    let style = if is_active {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let label = " :recall ";
    let input_text = app.recall_input.as_str();

    let line = Line::from(vec![
        Span::styled(label, style.add_modifier(Modifier::BOLD)),
        Span::styled(input_text, Style::default().fg(Color::White)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Guided LLM Recall Query ")
        .border_style(style);

    let paragraph = Paragraph::new(line).block(block);
    f.render_widget(paragraph, area);

    if is_active {
        let cursor_x = area.x + 1 + label.len() as u16 + app.recall_cursor as u16;
        let cursor_y = area.y + 1;
        f.set_cursor_position((cursor_x, cursor_y));
    }
}
