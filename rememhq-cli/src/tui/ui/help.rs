//! Help cheat-sheet modal overlay.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Render the help cheat-sheet modal overlay.
pub fn draw_help_modal(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(70, 70, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Keyboard Shortcuts Cheat Sheet ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::Cyan));

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let section_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

    let lines = vec![
        Line::from(Span::styled("Navigation & Selection", section_style)),
        Line::from(vec![
            Span::styled("  ↑ / ↓  or  j / k ", key_style),
            Span::styled("Move memory selection up/down", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  PageUp / PageDown", key_style),
            Span::styled("   Jump selection by 10 items", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Home / End       ", key_style),
            Span::styled("   Jump to top / bottom of list", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Enter            ", key_style),
            Span::styled("   Inspect selected memory in detail pane", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Searching, Filtering & Guided Recall",
            section_style,
        )),
        Line::from(vec![
            Span::styled("  /                ", key_style),
            Span::styled("   Full-text FTS search input mode", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  :                ", key_style),
            Span::styled(
                "   Guided LLM recall query mode (:recall <query>)",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  t                ", key_style),
            Span::styled(
                "   Cycle type filter (All/Fact/Procedure/Pref/Decision/Obs)",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  s / S            ", key_style),
            Span::styled("   Cycle sort field / toggle ascending order", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("Views & Actions", section_style)),
        Line::from(vec![
            Span::styled("  Tab              ", key_style),
            Span::styled("   Switch pane (Browse -> Stats -> Monitor)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  m                ", key_style),
            Span::styled("   Jump to Consolidation Event Monitor", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  d                ", key_style),
            Span::styled(
                "   Archive selected memory (opens confirmation)",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  r                ", key_style),
            Span::styled("   Force immediate data & stats refresh", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  ? / h            ", key_style),
            Span::styled("   Toggle this help cheat sheet modal", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Esc / q          ", key_style),
            Span::styled("   Close modal / Back to browser / Quit", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or ? to close this help window",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, popup_area);
}

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
