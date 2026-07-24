//! Confirmation modal dialog overlay for destructive actions.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app::{App, ConfirmAction};

/// Render the confirmation modal dialog over the layout.
pub fn draw_confirm_modal(f: &mut Frame, app: &App, area: Rect) {
    let Some(action) = &app.confirm_action else {
        return;
    };

    let popup_area = centered_rect(65, 30, area);
    f.render_widget(Clear, popup_area);

    let (title, prompt, note, confirm_button_text, border_color) = match action {
        ConfirmAction::Archive(id) => (
            format!(" Confirm Archive — {} ", &id.to_string()[..8]),
            format!(
                "Are you sure you want to ARCHIVE memory {}?",
                &id.to_string()[..8]
            ),
            "Archived memories are soft-deleted and can be restored using [u] (Unarchive).",
            " [Y] Yes, Archive ",
            Color::Yellow,
        ),
        ConfirmAction::Delete(id) => (
            format!(" PERMANENT DELETE — {} ", &id.to_string()[..8]),
            format!(
                "Are you sure you want to PERMANENTLY DELETE memory {}?",
                &id.to_string()[..8]
            ),
            "WARNING: Hard delete permanently removes this record from SQLite and vector index!",
            " [Y] PERMANENT DELETE ",
            Color::Red,
        ),
        ConfirmAction::Decay(id, factor) => (
            format!(" Apply Decay — {} ", &id.to_string()[..8]),
            format!(
                "Apply decay factor {:.2} to memory {}?",
                factor,
                &id.to_string()[..8]
            ),
            "Decay reduces importance weighting based on age.",
            " [Y] Apply Decay ",
            Color::Magenta,
        ),
        ConfirmAction::BulkArchive(ids) => (
            format!(" Bulk Archive ({} items) ", ids.len()),
            format!(
                "Are you sure you want to BULK ARCHIVE {} selected memories?",
                ids.len()
            ),
            "Archived memories can be restored using [u] (Unarchive).",
            " [Y] Bulk Archive ",
            Color::Yellow,
        ),
        ConfirmAction::BulkDelete(ids) => (
            format!(" PERMANENT BULK DELETE ({} items) ", ids.len()),
            format!(
                "Are you sure you want to PERMANENTLY DELETE {} selected memories?",
                ids.len()
            ),
            "WARNING: Bulk hard delete permanently removes all selected records!",
            " [Y] PERMANENT BULK DELETE ",
            Color::Red,
        ),
        ConfirmAction::BulkDecay(ids, factor) => (
            format!(" Bulk Decay ({} items) ", ids.len()),
            format!(
                "Apply decay factor {:.2} to {} selected memories?",
                factor,
                ids.len()
            ),
            "Decay reduces importance weighting based on age.",
            " [Y] Bulk Decay ",
            Color::Magenta,
        ),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border_color));

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            prompt,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(note, Style::default().fg(Color::DarkGray))),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                confirm_button_text,
                Style::default()
                    .bg(border_color)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                " [N/Esc] Cancel ",
                Style::default().bg(Color::DarkGray).fg(Color::White),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, popup_area);
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
