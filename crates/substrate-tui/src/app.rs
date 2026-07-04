//! Application state — top-level struct shared by all TUI components.

use std::time::Instant;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::config::TuiConfig;
use crate::dispatch_client::{GatewayClient, ServiceStatus};
use crate::help::draw_help;
use crate::proccompose::{load_compositions, Composition};
use crate::statusbar::draw_statusbar;

/// Top-level application state.
pub struct App {
    /// Dashboard configuration (gateway URL, compose dir, etc.).
    pub config: TuiConfig,
    /// Whether the gateway is currently reachable.
    pub connected: bool,
    /// Live compositions loaded from the compose directory.
    pub compositions: Vec<Composition>,
    /// Most-recently polled HTTP health status for each service.
    ///
    /// Refreshed every 5 s via [`App::refresh_service_statuses`].
    pub service_statuses: Vec<ServiceStatus>,
    /// A2A tasks tracked by the gateway.
    // WIP: task tracking is not yet surfaced in the render path.
    #[allow(dead_code)]
    pub tasks: Vec<Task>,
    /// Instant the dashboard started (for uptime calculation).
    #[allow(dead_code)]
    startup: Instant,
    /// Currently selected service index in the table.
    pub selected_index: usize,
    /// Whether the help overlay is shown.
    pub show_help: bool,
    /// Ratatui table state for selection highlight.
    pub table_state: TableState,
}

impl App {
    /// Create a new app state from the given configuration.
    ///
    /// Compositions are loaded eagerly from `config.compose_dir`; if the
    /// directory is missing or empty the list is simply empty (no panic).
    pub fn new(config: TuiConfig) -> Self {
        let compositions = load_compositions(&config.compose_dir);
        let mut table_state = TableState::default();
        if !compositions.is_empty() {
            table_state.select(Some(0));
        }
        Self {
            config,
            connected: false,
            compositions,
            service_statuses: Vec::new(),
            tasks: Vec::new(),
            startup: Instant::now(),
            selected_index: 0,
            show_help: false,
            table_state,
        }
    }

    /// Reload compositions from the configured compose directory.
    ///
    /// Call this on a poll tick to pick up changes to the compose manifests.
    // Available for callers; not yet used in the current event loop path.
    #[allow(dead_code)]
    pub fn refresh_compositions(&mut self) {
        self.compositions = load_compositions(&self.config.compose_dir);
    }

    /// Probe every service in `self.compositions` and update `service_statuses`.
    ///
    /// Should be called every 5 s from the TUI event loop to provide fresh
    /// Running/Stopped/Unknown indicators without blocking the render thread.
    pub async fn refresh_service_statuses(&mut self) {
        self.service_statuses = GatewayClient::get_status(&self.compositions).await;
    }

    /// Number of running dispatch lanes across all compositions.
    pub fn active_lanes(&self) -> usize {
        self.compositions
            .iter()
            .flat_map(|c| &c.members)
            .filter(|m| {
                let s = m.state.to_lowercase();
                s == "running" || s == "working"
            })
            .count()
    }

    /// Human-readable uptime since the dashboard started.
    #[allow(dead_code)]
    pub fn formatted_uptime(&self) -> String {
        let secs = self.startup.elapsed().as_secs();
        if secs == 0 {
            return "<1s".into();
        }
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;
        if hours > 0 {
            format!("{hours}h {mins}m")
        } else if mins > 0 {
            format!("{mins}m {secs}s")
        } else {
            format!("{secs}s")
        }
    }

    /// Move selection down one row.
    pub fn select_next(&mut self) {
        if self.compositions.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.compositions.len();
        self.table_state.select(Some(self.selected_index));
    }

    /// Move selection up one row.
    pub fn select_prev(&mut self) {
        if self.compositions.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.compositions.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        self.table_state.select(Some(self.selected_index));
    }

    /// Toggle the help overlay.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Render the full UI into `frame`.
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Split: main content on top, status bar at bottom (1 line).
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        self.render_service_table(frame, chunks[0]);
        draw_statusbar(frame, chunks[1], self);

        if self.show_help {
            let help_area = centered_rect(60, 70, area);
            draw_help(frame, help_area);
        }
    }

    fn render_service_table(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["Service", "Status", "Members", "Uptime"].iter().map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        });
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = self
            .compositions
            .iter()
            .map(|comp| {
                let status_color = comp.status.state_style();
                Row::new(vec![
                    Cell::from(Span::raw(comp.name.clone())),
                    Cell::from(Span::styled(
                        comp.status.to_string(),
                        Style::default().fg(status_color),
                    )),
                    Cell::from(Span::raw(comp.members.len().to_string())),
                    Cell::from(Span::raw(comp.formatted_uptime())),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" substrate-tui — Compositions "),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }
}

/// A tracked A2A task.
// WIP: fields populated once gateway task polling is connected.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: String,
}

/// Return a centered [`Rect`] that is `percent_x` wide and `percent_y` tall
/// relative to `r`.
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
