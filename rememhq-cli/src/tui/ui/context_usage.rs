//! Context Usage visualization — renders a dot-grid heatmap of memory store
//! capacity with per-type breakdown, inspired by LLM context-window visualizers.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;

/// Soft capacity used for the dot-grid visualization.
/// This is purely cosmetic — remem has no hard limit on memory count.
const VISUAL_CAPACITY: usize = 10_000;

/// Dots per row in the grid.
const DOTS_PER_ROW: usize = 40;

/// Type-to-color mapping (same palette as the bar chart in stats.rs).
struct TypeSlot {
    label: &'static str,
    dot: &'static str,
    color: Color,
    count: usize,
}

/// Render the context usage dot-grid and legend panel.
pub fn draw_context_usage(f: &mut Frame, app: &App, area: Rect) {
    let border_style = Style::default().fg(Color::Cyan);

    let block = Block::default()
        .borders(Borders::TOP)
        .title(" Memory Store Usage ")
        .border_style(border_style);

    let Some(ref stats) = app.stats else {
        let loading = Paragraph::new(" Waiting for stats…")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(loading, area);
        return;
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    // --- Gather per-type counts ---
    let type_order = ["fact", "procedure", "preference", "decision", "observation"];
    let type_colors = [
        Color::Green,
        Color::Blue,
        Color::Magenta,
        Color::Yellow,
        Color::Cyan,
    ];
    let type_dots = ["◉", "◉", "◉", "◉", "◉"];

    let slots: Vec<TypeSlot> = type_order
        .iter()
        .zip(type_colors.iter())
        .zip(type_dots.iter())
        .map(|((label, color), dot)| TypeSlot {
            label,
            dot,
            color: *color,
            count: stats.by_type.get(*label).copied().unwrap_or(0),
        })
        .collect();

    let total = stats.total_memories;
    let cap = VISUAL_CAPACITY.max(total); // expand if actual > soft cap
    let ratio = if cap > 0 {
        (total as f64 / cap as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let pct = (ratio * 100.0) as u32;

    // --- Layout: left (dot grid) | right (legend) ---
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // Dot grid
            Constraint::Percentage(45), // Legend
        ])
        .split(inner);

    // --- Build dot grid ---
    // Total dots to render (capped so the grid fits on screen)
    let available_rows = cols[0].height as usize;
    let total_dots = DOTS_PER_ROW * available_rows;
    let used_dots = ((total as f64 / cap as f64) * total_dots as f64).round() as usize;
    let used_dots = used_dots.min(total_dots);

    // Distribute used_dots among types proportionally
    let mut type_dot_counts: Vec<usize> = slots
        .iter()
        .map(|s| {
            if total > 0 {
                ((s.count as f64 / total as f64) * used_dots as f64).round() as usize
            } else {
                0
            }
        })
        .collect();

    // Fix rounding: ensure sum == used_dots
    let assigned: usize = type_dot_counts.iter().sum();
    if assigned < used_dots && !type_dot_counts.is_empty() {
        type_dot_counts[0] += used_dots - assigned;
    } else if assigned > used_dots && !type_dot_counts.is_empty() {
        let excess = assigned - used_dots;
        type_dot_counts[0] = type_dot_counts[0].saturating_sub(excess);
    }

    // Build flat dot list: each dot has a color
    let mut dot_list: Vec<(Color, &str)> = Vec::with_capacity(total_dots);
    for (i, slot) in slots.iter().enumerate() {
        for _ in 0..type_dot_counts[i] {
            dot_list.push((slot.color, slot.dot));
        }
    }
    // Fill remaining with empty dots
    while dot_list.len() < total_dots {
        dot_list.push((Color::DarkGray, "□"));
    }

    // Render rows
    let grid_lines: Vec<Line> = dot_list
        .chunks(DOTS_PER_ROW)
        .map(|row| {
            let spans: Vec<Span> = row
                .iter()
                .flat_map(|(color, dot)| {
                    vec![
                        Span::styled(*dot, Style::default().fg(*color)),
                        Span::raw(" "),
                    ]
                })
                .collect();
            Line::from(spans)
        })
        .collect();

    let grid = Paragraph::new(grid_lines);
    f.render_widget(grid, cols[0]);

    // --- Build legend panel ---
    let db_size = format_bytes(stats.db_size_bytes);
    let mut legend: Vec<Line> = Vec::new();

    // Header
    legend.push(Line::from(vec![
        Span::styled(
            " remem ",
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} memories", total),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    legend.push(Line::from(vec![Span::styled(
        format!("{}% of visual capacity ({})", pct, format_count(cap)),
        Style::default().fg(Color::DarkGray),
    )]));

    legend.push(Line::from(""));

    // Per-type breakdown
    legend.push(Line::from(Span::styled(
        " Memory distribution by type",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));

    for slot in &slots {
        let type_pct = if total > 0 {
            (slot.count as f64 / total as f64 * 100.0) as u32
        } else {
            0
        };
        legend.push(Line::from(vec![
            Span::styled(format!(" {} ", slot.dot), Style::default().fg(slot.color)),
            Span::styled(
                format!("{:<12}", capitalize(slot.label)),
                Style::default().fg(slot.color),
            ),
            Span::styled(
                format!("{:>6}", format_count(slot.count)),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({}%)", type_pct),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    legend.push(Line::from(""));

    // Storage & importance
    legend.push(Line::from(vec![
        Span::styled(" ⛁ ", Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("DB size: {}", db_size),
            Style::default().fg(Color::White),
        ),
    ]));

    legend.push(Line::from(vec![
        Span::styled(" ★ ", Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("Avg importance: {:.2}/10.0", stats.avg_importance),
            Style::default().fg(Color::White),
        ),
    ]));

    legend.push(Line::from(""));

    // Free capacity
    let free = cap.saturating_sub(total);
    let free_pct = if cap > 0 {
        (free as f64 / cap as f64 * 100.0) as u32
    } else {
        100
    };
    legend.push(Line::from(vec![
        Span::styled(" □ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("Free capacity: {} ({}%)", format_count(free), free_pct),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let legend_widget = Paragraph::new(legend);
    f.render_widget(legend_widget, cols[1]);
}

/// Format a large count with K/M suffix.
fn format_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
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

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + c.as_str(),
    }
}
