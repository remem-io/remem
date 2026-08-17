//! Inline Memory Editor modal overlay widget.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;

/// Render the Inline Memory Editor modal dialog overlay.
pub fn draw_edit_modal(f: &mut Frame, app: &App, area: Rect) {
    let Some(record) = &app.edit_record else {
        return;
    };

    let popup_area = centered_rect(75, 70, area);
    f.render_widget(Clear, popup_area);

    let title = format!(" ✏️ Edit Memory Record — {} ", &record.id.to_string()[..8]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::Yellow));

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Content area (field 0)
            Constraint::Length(3), // MemoryType (field 1)
            Constraint::Length(3), // Importance (field 2)
            Constraint::Length(3), // Tags (field 3)
            Constraint::Length(3), // Action bar
        ])
        .margin(1)
        .split(popup_area);

    f.render_widget(block, popup_area);

    // --- Field 0: Content ---
    let is_field0 = app.edit_focus_field == 0;
    let field0_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" [1] Memory Content ")
        .border_style(if is_field0 {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let content_para = Paragraph::new(app.edit_content_input.as_str())
        .block(field0_block)
        .wrap(Wrap { trim: false });
    f.render_widget(content_para, inner_chunks[0]);

    // --- Field 1: Memory Type ---
    let is_field1 = app.edit_focus_field == 1;
    let field1_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" [2] Memory Type (press Space/t to cycle) ")
        .border_style(if is_field1 {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let type_para = Paragraph::new(record.memory_type.to_string())
        .block(field1_block)
        .style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(type_para, inner_chunks[1]);

    // --- Field 2: Importance ---
    let is_field2 = app.edit_focus_field == 2;
    let field2_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" [3] Importance (1.0 to 10.0) ")
        .border_style(if is_field2 {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let imp_para = Paragraph::new(app.edit_importance_input.as_str()).block(field2_block);
    f.render_widget(imp_para, inner_chunks[2]);

    // --- Field 3: Tags ---
    let is_field3 = app.edit_focus_field == 3;
    let field3_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" [4] Tags (comma-separated) ")
        .border_style(if is_field3 {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let tags_para = Paragraph::new(app.edit_tags_input.as_str()).block(field3_block);
    f.render_widget(tags_para, inner_chunks[3]);

    // --- Action Bar ---
    let action_line = Line::from(vec![
        Span::styled(" [Tab] Next Field  ", Style::default().fg(Color::Yellow)),
        Span::styled(
            " [Enter] Save Changes  ",
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " [Esc] Cancel ",
            Style::default().bg(Color::DarkGray).fg(Color::White),
        ),
    ]);

    let action_para = Paragraph::new(action_line).alignment(Alignment::Center);
    f.render_widget(action_para, inner_chunks[4]);
}

/// Compute a centered Rect of given width and height percentage.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
