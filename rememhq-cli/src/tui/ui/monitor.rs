//! Real-time streaming agent monitor pane — renders live ReasoningEvent stream
//! with active agent sessions, timestamped entries, event rate indicator, per-type counters,
//! sparkline throughput graph, scroll-lock control, and plain-text status indicators.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Sparkline},
    Frame,
};
use rememhq_core::reasoning::ReasoningEvent;

use crate::tui::app::{App, Mode};

/// Render the real-time streaming monitor panel.
pub fn draw_monitor(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == Mode::Monitor;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = outer_block.inner(area);
    f.render_widget(outer_block, area);

    // Vertical layout: Header (3) | Active Agents Bar (2) | Sparkline (3) | Event Log (rest)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header: title + counters + rate
            Constraint::Length(2), // Active AI Agents Bar
            Constraint::Length(3), // Sparkline throughput
            Constraint::Min(5),    // Event log
        ])
        .split(inner);

    draw_header(f, app, chunks[0]);
    draw_active_agents(f, app, chunks[1]);
    draw_sparkline(f, app, chunks[2]);
    draw_event_log(f, app, chunks[3]);
}

/// Header: title bar with event count, rate, scroll-lock status, and per-type counters.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let total = app.consolidation_log.len();
    let rate = app.event_rate();
    let scroll_indicator = if app.monitor_auto_scroll {
        "[LIVE]"
    } else {
        "[PAUSED]"
    };

    let pulse = if rate > 0.0 {
        Span::styled(
            "[*]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("[ ]", Style::default().fg(Color::DarkGray))
    };

    let title_line = Line::from(vec![
        Span::styled(
            " Live Agent Stream & Monitor ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        pulse,
        Span::raw(" "),
        Span::styled(
            format!("{} events", total),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:.1} evt/s", rate),
            Style::default().fg(if rate > 1.0 {
                Color::Green
            } else if rate > 0.0 {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
        Span::raw("  "),
        Span::styled(
            scroll_indicator,
            Style::default().fg(if app.monitor_auto_scroll {
                Color::Green
            } else {
                Color::Yellow
            }),
        ),
    ]);

    // Per-type counter line
    let type_order = [
        "start", "fact", "clash", "graph", "done", "think", "tool", "obs", "recall",
    ];
    let type_colors = [
        Color::Green,
        Color::White,
        Color::Yellow,
        Color::Cyan,
        Color::Green,
        Color::LightMagenta,
        Color::LightCyan,
        Color::Blue,
        Color::Yellow,
    ];

    let mut counter_spans: Vec<Span> = vec![Span::raw(" ")];
    for (label, color) in type_order.iter().zip(type_colors.iter()) {
        let count = app.monitor_type_counts.get(*label).copied().unwrap_or(0);
        if count > 0 {
            counter_spans.push(Span::styled(
                format!("{}:{} ", label, count),
                Style::default().fg(*color),
            ));
        }
    }

    let counter_line = Line::from(counter_spans);

    let paragraph = Paragraph::new(vec![title_line, counter_line]);
    f.render_widget(paragraph, area);
}

/// Active AI Agents Status Bar: shows working agents (Claude Code, Codex, Antigravity CLI, Cursor, etc.).
fn draw_active_agents(f: &mut Frame, app: &App, area: Rect) {
    let now = std::time::Instant::now();

    let mut spans: Vec<Span> = vec![Span::styled(
        " Active Agents: ",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )];

    if app.active_agent_sessions.is_empty() {
        spans.push(Span::styled(
            "None (Waiting for agent connections...)",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        let mut sorted_agents: Vec<(&String, &(&'static str, std::time::Instant))> =
            app.active_agent_sessions.iter().collect();
        sorted_agents.sort_by_key(|(_, (_, ts))| *ts);
        sorted_agents.reverse();

        for (sid, (agent_name, last_seen)) in sorted_agents.iter().take(5) {
            let age_secs = now.duration_since(*last_seen).as_secs();
            let (status_text, style) = if age_secs < 5 {
                (
                    format!("ACTIVE {}s ago", age_secs),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else if age_secs < 30 {
                (
                    format!("IDLE {}s ago", age_secs),
                    Style::default().fg(Color::Yellow),
                )
            } else {
                (
                    format!("{}s ago", age_secs),
                    Style::default().fg(Color::DarkGray),
                )
            };

            let short_sid = if sid.len() > 8 { &sid[..8] } else { sid };

            spans.push(Span::styled(
                format!("[{} ({}) - {}]  ", agent_name, short_sid, status_text),
                style,
            ));
        }
    }

    let paragraph = Paragraph::new(Line::from(spans));
    f.render_widget(paragraph, area);
}

/// Sparkline throughput graph: events per tick over the last 30 ticks.
fn draw_sparkline(f: &mut Frame, app: &App, area: Rect) {
    let data: Vec<u64> = app.monitor_sparkline.iter().copied().collect();

    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Throughput (events/tick) ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(&data)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(sparkline, area);
}

/// Event log: scrollable list of timestamped events.
fn draw_event_log(f: &mut Frame, app: &App, area: Rect) {
    let now = std::time::Instant::now();
    let visible_height = area.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = app
        .consolidation_log
        .iter()
        .rev()
        .skip(app.monitor_scroll)
        .take(visible_height)
        .map(|ts_event| {
            let age = now.duration_since(ts_event.received_at);
            let age_str = format_age(age);
            let age_color = if age.as_secs() < 2 {
                Color::Green
            } else if age.as_secs() < 10 {
                Color::Yellow
            } else {
                Color::DarkGray
            };

            let (prefix, content, color) = format_event(&ts_event.event);

            let line = Line::from(vec![
                Span::styled(format!("{:>6} ", age_str), Style::default().fg(age_color)),
                Span::styled(
                    format!("[{}] ", prefix),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(content, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let scroll_info = if app.monitor_auto_scroll {
        String::new()
    } else {
        format!(" [scroll: +{}] ", app.monitor_scroll)
    };

    let title = format!(" Event Log{} ", scroll_info);

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, area);
}

/// Format a ReasoningEvent into (prefix, content, color) without emojis.
fn format_event(event: &ReasoningEvent) -> (&'static str, String, Color) {
    match event {
        ReasoningEvent::AgentConnected {
            agent_name,
            agent_version,
            ..
        } => (
            "CONN ",
            format!("{} v{} connected", agent_name, agent_version),
            Color::Green,
        ),
        ReasoningEvent::AgentDisconnected { agent_name, .. } => (
            "DISC ",
            format!("{} disconnected", agent_name),
            Color::DarkGray,
        ),
        ReasoningEvent::ConsolidationStarted { session_id } => {
            ("START", format!("session {}", session_id), Color::Green)
        }
        ReasoningEvent::FactExtracted { content } => ("FACT ", content.clone(), Color::White),
        ReasoningEvent::ContradictionDetected {
            existing_id,
            new_content,
        } => (
            "CLASH",
            format!("{} -> {}", &existing_id.to_string()[..8], new_content),
            Color::Yellow,
        ),
        ReasoningEvent::KnowledgeTripleFound {
            subject,
            predicate,
            object,
        } => (
            "GRAPH",
            format!("{} -{}-> {}", subject, predicate, object),
            Color::Cyan,
        ),
        ReasoningEvent::ConsolidationCompleted {
            session_id: _,
            new_facts,
        } => (
            "DONE ",
            format!("{} new facts extracted", new_facts),
            Color::Green,
        ),
        ReasoningEvent::ThinkingDelta {
            session_id: _,
            thought,
        } => ("THINK", thought.clone(), Color::LightMagenta),
        ReasoningEvent::ToolCall {
            session_id: _,
            tool_name,
            input_summary,
        } => (
            "TOOL ",
            format!("{}({})", tool_name, input_summary),
            Color::LightCyan,
        ),
        ReasoningEvent::ObservationStreamed {
            session_id: _,
            observation_type,
            content,
        } => (
            "OBS  ",
            format!("[{}] {}", observation_type, content),
            Color::Blue,
        ),
        ReasoningEvent::MemoryRecalled {
            session_id: _,
            query,
            count,
        } => (
            "RECALL",
            format!("'{}' -> {} results", query, count),
            Color::Yellow,
        ),
    }
}

/// Format a Duration into a human-readable relative age string.
fn format_age(age: std::time::Duration) -> String {
    let secs = age.as_secs();
    if secs < 1 {
        "now".to_string()
    } else if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}
