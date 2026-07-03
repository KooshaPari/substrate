//! Dashboard — main dispatch overview and process composition monitor.
//!
//! Layout (top → bottom):
//! - Header row: gateway health · lane count · task count · uptime
//! - Main area (left/right split):
//!   - Left: dispatch lanes table (compositions + members)
//!   - Right: A2A tasks list (id, status, description)
//! - Footer: proc-compose composition gauge

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table},
    Frame,
};

use crate::app::{summarise_tasks, App};

/// Draw the full dashboard inside `area`.
pub fn draw_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header
            Constraint::Min(6),    // main pane (lanes + tasks)
            Constraint::Length(3), // proc-compose footer
        ])
        .split(area);

    draw_header(frame, outer[0], app);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(outer[1]);

    draw_lanes_table(frame, main[0], app);
    draw_tasks_panel(frame, main[1], app);
    draw_compose_footer(frame, outer[2], app);
}

// ── header ───────────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // health + gateway URL
            Constraint::Percentage(20), // lanes
            Constraint::Percentage(25), // tasks summary
            Constraint::Percentage(25), // uptime
        ])
        .split(area);

    let (health_color, health_text) = if app.connected {
        (Color::Green, "● CONNECTED")
    } else {
        (Color::Red, "○ DISCONNECTED")
    };

    let health = Paragraph::new(Line::from(vec![
        Span::styled(
            health_text,
            Style::default()
                .fg(health_color)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            &app.config.gateway_url,
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    frame.render_widget(health, cols[0]);

    let lanes = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("LANES  {}", app.active_lanes()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::raw(" active"),
    ]));
    frame.render_widget(lanes, cols[1]);

    let summary = summarise_tasks(&app.tasks);
    let task_text = format!(
        "TASKS  {}  wk:{} ok:{} fail:{}",
        summary.total, summary.working, summary.completed, summary.failed
    );
    let tasks_widget = Paragraph::new(Line::from(Span::styled(
        task_text,
        Style::default().fg(Color::Magenta),
    )));
    frame.render_widget(tasks_widget, cols[2]);

    let uptime = Paragraph::new(Line::from(vec![
        Span::styled("UPTIME  ", Style::default().fg(Color::DarkGray)),
        Span::styled(app.formatted_uptime(), Style::default().fg(Color::Yellow)),
    ]));
    frame.render_widget(uptime, cols[3]);
}

// ── dispatch lanes table ──────────────────────────────────────────────────

fn draw_lanes_table(frame: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["LANE ID", "STATE", "ENGINE", "MODEL", "UPTIME", "PROMPT"]
        .iter()
        .map(|h| {
            Cell::from(Line::from(Span::styled(
                *h,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )))
        });
    let header = Row::new(header_cells)
        .style(Style::default().bg(Color::Rgb(40, 40, 40)))
        .height(1);

    let rows: Vec<Row> = app
        .compositions
        .iter()
        .flat_map(|comp| {
            let comp_row = Row::new(vec![
                Cell::from(format!("[{}]", comp.name)),
                Cell::from("─"),
                Cell::from(format!("{} members", comp.members.len())),
                Cell::from("─"),
                Cell::from(comp.formatted_uptime()),
                Cell::from(comp.status.to_string()),
            ])
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            );

            let member_rows: Vec<Row> = comp
                .members
                .iter()
                .map(|m| {
                    let state_color = m.state_style();
                    let cell = |s: String| {
                        if s.is_empty() {
                            Cell::from("─")
                        } else {
                            Cell::from(s)
                        }
                    };
                    Row::new(vec![
                        cell(m.short_id()),
                        Cell::from(Line::from(Span::styled(
                            &m.state,
                            Style::default().fg(state_color),
                        ))),
                        cell(m.engine.clone()),
                        cell(m.model.clone()),
                        cell(m.formatted_uptime()),
                        cell(m.prompt_preview.clone()),
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
            Constraint::Min(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Dispatch Lanes ")
            .title_alignment(Alignment::Left),
    );

    frame.render_widget(table, area);
}

// ── A2A tasks panel ───────────────────────────────────────────────────────

fn draw_tasks_panel(frame: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from(Span::styled(
            "STATUS",
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "ID",
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "DESCRIPTION",
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
    ])
    .style(Style::default().bg(Color::Rgb(40, 40, 40)))
    .height(1);

    let rows: Vec<Row> = app
        .tasks
        .iter()
        .map(|t| {
            let status_color = match t.status.to_lowercase().as_str() {
                "working" | "in_progress" | "running" => Color::Green,
                "completed" | "done" | "succeeded" => Color::Cyan,
                "failed" | "error" | "canceled" => Color::Red,
                _ => Color::DarkGray,
            };
            let short_id = if t.id.len() >= 8 { &t.id[..8] } else { &t.id };
            Row::new(vec![
                Cell::from(Span::styled(&t.status, Style::default().fg(status_color))),
                Cell::from(short_id.to_owned()),
                Cell::from(t.description.clone()),
            ])
            .style(Style::default().fg(Color::White))
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" A2A Tasks ")
            .title_alignment(Alignment::Left),
    );

    frame.render_widget(table, area);
}

// ── proc-compose footer ───────────────────────────────────────────────────

fn draw_compose_footer(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let comps_text = Paragraph::new(Line::from(Span::styled(
        format!("COMPOSITIONS  {}", app.compositions.len()),
        Style::default()
            .fg(Color::Green)
            .add_modifier(ratatui::style::Modifier::BOLD),
    )))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Proc-Compose "),
    );
    frame.render_widget(comps_text, cols[0]);

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
