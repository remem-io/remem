//! Phase-2: live consolidation monitor pane.
//!
//! Renders the ReasoningEvent ring buffer as a scrolling log.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use rememhq_core::reasoning::ReasoningEvent;

use crate::tui::app::{App, Mode};

/// Render the consolidation monitor log.
pub fn draw_monitor(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == Mode::Monitor;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .consolidation_log
        .iter()
        .rev()
        .map(|event| {
            let (prefix, content, color) = match event {
                ReasoningEvent::ConsolidationStarted { session_id } => {
                    ("▶ START", format!("session {}", session_id), Color::Green)
                }
                ReasoningEvent::FactExtracted { content } => {
                    ("  FACT ", content.clone(), Color::White)
                }
                ReasoningEvent::ContradictionDetected {
                    existing_id,
                    new_content,
                } => (
                    "⚠ CLASH",
                    format!("{} → {}", &existing_id.to_string()[..8], new_content),
                    Color::Yellow,
                ),
                ReasoningEvent::KnowledgeTripleFound {
                    subject,
                    predicate,
                    object,
                } => (
                    "  GRAPH",
                    format!("{} —{}→ {}", subject, predicate, object),
                    Color::Cyan,
                ),
                ReasoningEvent::ConsolidationCompleted {
                    session_id: _,
                    new_facts,
                } => ("✓ DONE ", format!("{} new facts", new_facts), Color::Green),
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("[{}] ", prefix),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(content, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = app.consolidation_log.len();
    let title = format!(" Consolidation Monitor ({}) ", count);

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style),
    );
    f.render_widget(list, area);
}
