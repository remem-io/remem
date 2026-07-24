//! Keyboard cheat-sheet overlay modal dialog.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::app::App;

/// Render the help cheat-sheet modal overlay over the layout.
pub fn draw_help_modal(f: &mut Frame, _app: &App, area: Rect) {
    let popup_area = centered_rect(72, 80, area);
    f.render_widget(Clear, popup_area);

    let title = " Remem TUI Keyboard Cheat-Sheet ";

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(Color::Cyan));

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let category_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let content = vec![
        Line::from(Span::styled("Navigation & Views", category_style)),
        Line::from(vec![
            Span::styled("  Tab / Shift+Tab  ", key_style),
            Span::styled(
                "Cycle focus (Browse → Detail → Stats → Monitor)",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  b / d / s / m    ", key_style),
            Span::styled(
                "Jump to Browse / Detail / Stats+ContextUsage / Monitor pane",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  ↑/↓, j/k, Home/End", key_style),
            Span::styled("Navigate rows in browser / results list", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  [ / ] or ← / →   ", key_style),
            Span::styled("Previous / Next page in browser", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("Memory Creation & Editing", category_style)),
        Line::from(vec![
            Span::styled("  n                ", key_style),
            Span::styled("Open 'New Memory' creation modal", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  e                ", key_style),
            Span::styled(
                "Open inline Memory Editor modal on selected record",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  v                ", key_style),
            Span::styled("View Memory Version History & diff timeline", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Search, Recall & Command Palette",
            category_style,
        )),
        Line::from(vec![
            Span::styled("  /                ", key_style),
            Span::styled("Live search-as-you-type (FTS full-text search)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  :                ", key_style),
            Span::styled("Universal Command Palette (↑/↓ history)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("     :recall <q>   ", key_style),
            Span::styled("Guided LLM recall query", desc_style),
        ]),
        Line::from(vec![
            Span::styled("     :search <q>   ", key_style),
            Span::styled("FTS search query", desc_style),
        ]),
        Line::from(vec![
            Span::styled("     :filter <type>", key_style),
            Span::styled("Filter by type (fact/proc/pref/dec/obs)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("     :sort <field> ", key_style),
            Span::styled("Sort by field (importance/decay/created)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("     :clear        ", key_style),
            Span::styled("Clear all filters & search input", desc_style),
        ]),
        Line::from(vec![
            Span::styled("     :q / :quit    ", key_style),
            Span::styled("Quit the TUI", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  t                ", key_style),
            Span::styled(
                "Cycle MemoryType filter (all, fact, pref, etc.)",
                desc_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("  o / O            ", key_style),
            Span::styled("Cycle sort field / toggle sort direction", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Graph, Sessions & Multi-Select",
            category_style,
        )),
        Line::from(vec![
            Span::styled("  g                ", key_style),
            Span::styled("Knowledge Graph & Entity Browser", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  S                ", key_style),
            Span::styled("Session Transcript & Summary Viewer", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Space            ", key_style),
            Span::styled("Toggle row multi-selection", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Shift+A / X / C  ", key_style),
            Span::styled("Bulk archive / delete / decay selected", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Esc              ", key_style),
            Span::styled("Clear selection set (in Browse mode)", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("Memory Lifecycle & Actions", category_style)),
        Line::from(vec![
            Span::styled("  Enter            ", key_style),
            Span::styled("Inspect highlighted memory in Detail pane", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  d / a            ", key_style),
            Span::styled("Archive memory (soft delete)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  D / x            ", key_style),
            Span::styled("PERMANENT hard delete memory", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  u                ", key_style),
            Span::styled("Unarchive / restore memory", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  c                ", key_style),
            Span::styled("Apply decay factor", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  r                ", key_style),
            Span::styled("Refresh data and stats from SQLite", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("General", category_style)),
        Line::from(vec![
            Span::styled("  ? / h            ", key_style),
            Span::styled("Toggle this help cheat-sheet", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Esc / q          ", key_style),
            Span::styled("Close modal / Quit TUI", desc_style),
        ]),
    ];

    let paragraph = Paragraph::new(content).block(block);
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
