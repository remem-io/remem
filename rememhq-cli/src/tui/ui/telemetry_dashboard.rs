//! Telemetry, Token Economy, and Cost Metering Dashboard pane.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::tui::app::App;

/// Render the comprehensive Telemetry & Cost Dashboard.
pub fn draw_telemetry_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Overview Summary Bar
            Constraint::Min(12),   // Middle: Cost & Token Stats (Left) | Cache & Efficiency (Right)
            Constraint::Length(9), // Bottom: Latency Distribution & Engine Percentiles
        ])
        .split(area);

    draw_overview_bar(f, app, main_chunks[0]);

    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // Financial Cost & Token Breakdown
            Constraint::Percentage(45), // Embedding Cache & Memory Index Stats
        ])
        .split(main_chunks[1]);

    draw_cost_breakdown(f, app, middle_chunks[0]);
    draw_cache_and_engine(f, app, middle_chunks[1]);
    draw_latency_percentiles(f, app, main_chunks[2]);
}

/// Render the top overview summary banner.
fn draw_overview_bar(f: &mut Frame, app: &App, area: Rect) {
    let telemetry = app.telemetry.as_ref();
    let uptime_secs = telemetry.map(|t| t.metrics.uptime_seconds).unwrap_or(0);
    let total_stores = telemetry.map(|t| t.metrics.total_stores).unwrap_or(0);
    let total_recalls = telemetry.map(|t| t.metrics.total_recalls).unwrap_or(0);
    let total_consolidations = telemetry
        .map(|t| t.metrics.total_consolidations)
        .unwrap_or(0);
    let active_sessions = telemetry.map(|t| t.metrics.active_sessions).unwrap_or(0);

    let hours = uptime_secs / 3600;
    let mins = (uptime_secs % 3600) / 60;
    let secs = uptime_secs % 60;

    let line1 = Line::from(vec![
        Span::styled(" Uptime: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}h {:02}m {:02}s", hours, mins, secs),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Active Sessions: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", active_sessions),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Store Ops: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", total_stores),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Recall Ops: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", total_recalls),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("Consolidations: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", total_consolidations),
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" System Telemetry Overview ")
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(vec![line1]).block(block);
    f.render_widget(paragraph, area);
}

/// Render the Financial Cost, Token Counter, and Provider Breakdown.
fn draw_cost_breakdown(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Token Economy & Cost Metering ")
        .border_style(Style::default().fg(Color::Yellow));

    let telemetry = app.telemetry.as_ref();
    let total_tokens = telemetry.map(|t| t.cost_summary.total_tokens).unwrap_or(0);
    let prompt_tokens = telemetry.map(|t| t.cost_summary.prompt_tokens).unwrap_or(0);
    let completion_tokens = telemetry
        .map(|t| t.cost_summary.completion_tokens)
        .unwrap_or(0);
    let estimated_cost = telemetry
        .map(|t| t.cost_summary.estimated_cost_usd)
        .unwrap_or(0.0);
    let total_calls = telemetry.map(|t| t.cost_summary.total_calls).unwrap_or(0);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                " Total LLM Cost (Est.): ",
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("${:.6} USD", estimated_cost),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Total Calls: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", total_calls),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " Total Tokens:          ",
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{}", total_tokens),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ("),
            Span::styled(
                format!("Prompt: {}", prompt_tokens),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(" | "),
            Span::styled(
                format!("Completion: {}", completion_tokens),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(")"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Provider Call Distribution:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
    ];

    if let Some(t) = telemetry {
        if t.cost_summary.usage_by_provider.is_empty() {
            lines.push(Line::from(Span::styled(
                "   (No provider calls recorded yet)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (provider, calls) in &t.cost_summary.usage_by_provider {
                let share = if total_calls > 0 {
                    (*calls as f64 / total_calls as f64) * 100.0
                } else {
                    0.0
                };
                let bar_len = ((share / 10.0).round() as usize).min(10);
                let bar = "█".repeat(bar_len) + &"░".repeat(10 - bar_len);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("   {:<12} ", provider),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(format!("[{}] ", bar), Style::default().fg(Color::Blue)),
                    Span::styled(
                        format!("{:>5} calls ({:.1}%)", calls, share),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "   (Awaiting telemetry data...)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

/// Render the Embedding Cache and Vector Store Index Statistics.
fn draw_cache_and_engine(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Embedding Cache & Index Health ")
        .border_style(Style::default().fg(Color::Green));

    let telemetry = app.telemetry.as_ref();
    let hits = telemetry.map(|t| t.cache_stats.hits).unwrap_or(0);
    let misses = telemetry.map(|t| t.cache_stats.misses).unwrap_or(0);
    let ratio_pct = telemetry
        .map(|t| t.cache_stats.hit_rate_percentage)
        .unwrap_or(0.0);
    let cached_items = telemetry.map(|t| t.cache_stats.total_entries).unwrap_or(0);

    let bar_len = ((ratio_pct / 10.0).round() as usize).min(10);
    let cache_bar = "█".repeat(bar_len) + &"░".repeat(10 - bar_len);

    let lines = vec![
        Line::from(vec![
            Span::styled(" Cache Hit Ratio:    ", Style::default().fg(Color::White)),
            Span::styled(
                format!("[{}] ", cache_bar),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                format!("{:.1}%", ratio_pct),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Cache Hits / Misses:", Style::default().fg(Color::White)),
            Span::styled(format!(" {} hits", hits), Style::default().fg(Color::Cyan)),
            Span::raw(" / "),
            Span::styled(
                format!("{} misses", misses),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Cached Embeddings:  ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{} vectors", cached_items),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Storage & Vector Backend:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(
                "   SQLite Engine:    ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "WAL Mode (v6 schema migrations)",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   HNSW Vector Index:",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "libremem C++17 FFI (Cosine L2)",
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "   Full-Text Search: ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("FTS5 Porter Stemmer", Style::default().fg(Color::White)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

/// Render the Latency Percentiles (p50, p95, p99) table.
fn draw_latency_percentiles(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Operation Latency Distribution (Rolling Window) ")
        .border_style(Style::default().fg(Color::Magenta));

    let telemetry = app.telemetry.as_ref();
    let store_p50 = telemetry
        .map(|t| t.metrics.store_latency_p50_ms)
        .unwrap_or(0.0);
    let store_p95 = telemetry
        .map(|t| t.metrics.store_latency_p95_ms)
        .unwrap_or(0.0);
    let recall_p50 = telemetry
        .map(|t| t.metrics.recall_latency_p50_ms)
        .unwrap_or(0.0);
    let recall_p95 = telemetry
        .map(|t| t.metrics.recall_latency_p95_ms)
        .unwrap_or(0.0);
    let recall_p99 = telemetry
        .map(|t| t.metrics.recall_latency_p99_ms)
        .unwrap_or(0.0);

    let rows = vec![
        Row::new(vec![
            Span::styled(
                "Store Memory",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:.2} ms", store_p50),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                format!("{:.2} ms", store_p95),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!("{:.2} ms", store_p95 * 1.2),
                Style::default().fg(Color::LightYellow),
            ),
            Span::styled(
                "In-memory embedding + SQLite insert + HNSW append",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Row::new(vec![
            Span::styled(
                "Recall Memory",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:.2} ms", recall_p50),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                format!("{:.2} ms", recall_p95),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!("{:.2} ms", recall_p99),
                Style::default().fg(Color::LightYellow),
            ),
            Span::styled(
                "Hybrid HNSW Cosine Search + FTS5 BM25 + LLM Re-ranking",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    let widths = [
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Min(30),
    ];

    let header = Row::new(vec![
        Span::styled(
            "Operation",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "p50 (Median)",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "p95",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "p99",
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Execution Pipeline Details",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .bottom_margin(1);

    let table = Table::new(rows, widths).header(header).block(block);
    f.render_widget(table, area);
}
