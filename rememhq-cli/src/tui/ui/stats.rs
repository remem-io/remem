//! Stats dashboard pane — renders StoreStats as gauges and charts.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Gauge, Paragraph},
    Frame,
};

use crate::tui::app::{App, Mode};

/// Render the stats dashboard.
pub fn draw_stats(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.mode == Mode::Stats;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Stats Dashboard ")
        .border_style(border_style);

    let Some(ref stats) = app.stats else {
        let loading = Paragraph::new(" Loading statistics…")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(loading, area);
        return;
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split vertically: summary text | importance gauge | type bar chart
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // summary text
            Constraint::Length(3), // importance gauge
            Constraint::Min(5),    // type breakdown chart
        ])
        .split(inner);

    // --- Summary text ---
    let db_size = format_bytes(stats.db_size_bytes);
    let summary_lines = vec![
        Line::from(vec![
            Span::styled(
                "Total Memories: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                stats.total_memories.to_string(),
                Style::default().fg(Color::White),
            ),
            Span::styled("    DB Size: ", Style::default().fg(Color::Yellow)),
            Span::styled(db_size, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Avg Importance: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:.2} / 10.0", stats.avg_importance),
                Style::default().fg(Color::White),
            ),
        ]),
    ];
    let summary = Paragraph::new(summary_lines);
    f.render_widget(summary, chunks[0]);

    // --- Importance gauge ---
    let importance_ratio = (stats.avg_importance / 10.0).clamp(0.0, 1.0) as f64;
    let gauge_color = if stats.avg_importance >= 7.0 {
        Color::Green
    } else if stats.avg_importance >= 4.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Avg Importance ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .gauge_style(Style::default().fg(gauge_color))
        .ratio(importance_ratio)
        .label(format!("{:.1}/10", stats.avg_importance));
    f.render_widget(gauge, chunks[1]);

    // --- Type breakdown bar chart ---
    let type_order = ["fact", "procedure", "preference", "decision", "observation"];
    let type_colors = [
        Color::Green,
        Color::Blue,
        Color::Magenta,
        Color::Yellow,
        Color::Cyan,
    ];

    let bars: Vec<Bar> = type_order
        .iter()
        .zip(type_colors.iter())
        .map(|(t, color)| {
            let count = stats.by_type.get(*t).copied().unwrap_or(0) as u64;
            Bar::default()
                .label(Line::from((*t).to_string()))
                .value(count)
                .style(Style::default().fg(*color))
        })
        .collect();

    let bar_chart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" By Type ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .data(BarGroup::default().bars(&bars))
        .bar_width(10)
        .bar_gap(2)
        .value_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(bar_chart, chunks[2]);
}

/// Format bytes into a human-readable string.
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
