// Scaffolding WIP: dashboard widget functions are defined but not yet wired
// into the event loop.  Suppress until the caller side is connected.
#![allow(dead_code, unused_imports)]
//! Dashboard — main dispatch overview and process composition monitor.
//!
//! Shows:
//! - Gateway health indicator (green/red dot)
//! - Dispatch lane table (id, state, engine, model, uptime)
//! - Process composition status (running compositions, their members, resource usage)
//! - Key metrics footer (tasks, healthy nodes, uptime)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;

/// Draw the full dashboard inside `area`.
pub fn draw_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header row (gateway health + key stats)
            Constraint::Min(6),    // dispatch lanes table
            Constraint::Length(3), // proc-compose mini summary
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_lanes_table(frame, chunks[1], app);
    draw_compose_summary(frame, chunks[2], app);
}

// ── header ───────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22), // health + gateway info
            Constraint::Length(22), // lanes count
            Constraint::Length(22), // tasks
            Constraint::Min(10),    // uptime
        ])
        .split(area);

    let (health_color, health_text) = if app.connected {
        (Color::Green, "● CONNECTED")
    } else {
        (Color::Red, "○ DISCONNECTED")
    };

    let health = Paragraph::new(Line::from(vec![
        Span::styled(health_text, Style::default().fg(health_color).bold()),
        Span::raw(" "),
        Span::styled(
            &app.config.gateway_url,
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::NONE));
    frame.render_widget(health, cols[0]);

    let lanes_count = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("LANES  {}", app.active_lanes()),
            Style::default().fg(Color::Cyan).bold(),
        ),
        Span::raw(" active"),
    ]));
    frame.render_widget(lanes_count, cols[1]);

    let tasks = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("TASKS  {}", app.tasks.len()),
            Style::default().fg(Color::Magenta).bold(),
        ),
        Span::raw(" tracked"),
    ]));
    frame.render_widget(tasks, cols[2]);

    let uptime = Paragraph::new(Line::from(vec![
        Span::styled("UPTIME ", Style::default().fg(Color::DarkGray)),
        Span::styled(app.formatted_uptime(), Style::default().fg(Color::Yellow)),
    ]));
    frame.render_widget(uptime, cols[3]);
}

// ── dispatch lanes table ─────────────────────────────────────────────

fn draw_lanes_table(frame: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["LANE ID", "STATE", "ENGINE", "MODEL", "UPTIME", "PROMPT"]
        .iter()
        .map(|h| {
            Cell::from(Line::from(Span::styled(
                *h,
                Style::default().bold().fg(Color::White),
            )))
        });
    let header = Row::new(header_cells)
        .style(Style::default().bg(Color::Rgb(40, 40, 40)))
        .height(1);

    let rows: Vec<Row> = app
        .compositions
        .iter()
        .flat_map(|comp| {
            let comp_style = Style::default().fg(Color::Cyan).bold();
            let comp_row = Row::new(vec![
                Cell::from(format!("[{}]", comp.name)),
                Cell::from("─"),
                Cell::from(format!("{} members", comp.members.len())),
                Cell::from("─"),
                Cell::from(comp.formatted_uptime()),
                Cell::from(comp.status.to_string()),
            ])
            .style(comp_style);

            let member_rows: Vec<Row> = comp
                .members
                .iter()
                .map(|m| {
                    let state_style = m.state_style();
                    let cell = |s: String| {
                        if s.is_empty() {
                            Cell::from("─")
                        } else {
                            Cell::from(s)
                        }
                    };
                    Row::new(vec![
                        cell(m.short_id()),
                        Cell::from(Line::from(Span::styled(&m.state, state_style))),
                        cell(m.engine.clone()),
                        cell(m.model.clone()),
                        cell(m.formatted_uptime()),
                        cell(m.prompt_preview().to_owned()),
                    ])
                    .style(Style::default().fg(Color::White))
                })
                .collect();

            std::iter::once(comp_row)
                .chain(member_rows)
                .collect::<Vec<_>>()
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Dispatch Lanes ")
            .title_alignment(ratatui::layout::Alignment::Left),
    );

    frame.render_widget(table, area);
}

// ── proc-compose mini summary ────────────────────────────────────────

fn draw_compose_summary(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Compositions count
    let comps = Paragraph::new(Line::from(vec![Span::styled(
        format!("COMPOSITIONS  {}", app.compositions.len()),
        Style::default().fg(Color::Green).bold(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Proc-Compose "),
    );
    frame.render_widget(comps, cols[0]);

    // Per-composition mini gauge
    if let Some(comp) = app.compositions.first() {
        let running = comp
            .members
            .iter()
            .filter(|m| m.state == "working" || m.state == "running")
            .count();
        let total = comp.members.len();
        let ratio = if total > 0 {
            running as f64 / total as f64
        } else {
            0.0
        };
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", comp.name)),
            )
            .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
            .ratio(ratio)
            .label(format!("{}/{} lanes active", running, total));
        frame.render_widget(gauge, cols[1]);
    }
}
