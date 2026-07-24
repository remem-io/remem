//! Phase-2 & Live Stream: consolidation & agent token/thinking monitor pane.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use rememhq_core::reasoning::ReasoningEvent;

use crate::tui::app::{App, Mode};

/// Render the live event stream monitor.
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
                    ("▶ START ", format!("session {}", session_id), Color::Green)
                }
                ReasoningEvent::FactExtracted { content } => {
                    ("💡 FACT  ", content.clone(), Color::White)
                }
                ReasoningEvent::ContradictionDetected {
                    existing_id,
                    new_content,
                } => (
                    "⚠️ CLASH ",
                    format!("{} → {}", &existing_id.to_string()[..8], new_content),
                    Color::Yellow,
                ),
                ReasoningEvent::KnowledgeTripleFound {
                    subject,
                    predicate,
                    object,
                } => (
                    "🌐 GRAPH ",
                    format!("{} —{}→ {}", subject, predicate, object),
                    Color::Cyan,
                ),
                ReasoningEvent::ConsolidationCompleted {
                    session_id: _,
                    new_facts,
                } => (
                    "✓ DONE  ",
                    format!("{} new facts extracted", new_facts),
                    Color::Green,
                ),
                ReasoningEvent::ThinkingDelta {
                    session_id: _,
                    thought,
                } => ("🧠 THINK ", thought.clone(), Color::LightMagenta),
                ReasoningEvent::ToolCall {
                    session_id: _,
                    tool_name,
                    input_summary,
                } => (
                    "⚙️ TOOL  ",
                    format!("{}({})", tool_name, input_summary),
                    Color::LightCyan,
                ),
                ReasoningEvent::ObservationStreamed {
                    session_id: _,
                    observation_type,
                    content,
                } => (
                    "👁 OBS   ",
                    format!("[{}] {}", observation_type, content),
                    Color::Blue,
                ),
                ReasoningEvent::MemoryRecalled {
                    session_id: _,
                    query,
                    count,
                } => (
                    "🔍 RECALL",
                    format!("'{}' → {} results", query, count),
                    Color::Yellow,
                ),
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
    let title = format!(" Live Agent Stream & Monitor ({}) ", count);

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style),
    );
    f.render_widget(list, area);
}
