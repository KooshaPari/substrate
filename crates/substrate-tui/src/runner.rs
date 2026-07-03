//! Shared async event loop — imported by both `main.rs` (binary) and
//! `lib.rs` (library used by `driver-cli dash`).

use std::io;
use std::time::Duration;

use anyhow::Context;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::app::{tasks_from_wire, App};
use crate::config::TuiConfig;
use crate::dispatch_client::GatewayClient;
use crate::proccompose::load_compositions;

/// Terminal events sent from background pollers.
enum Tick {
    /// Data refresh — update the app state.
    Data {
        connected: bool,
        compositions: Vec<crate::proccompose::Composition>,
        tasks: Vec<crate::app::Task>,
    },
    /// Crossterm input event.
    Input(Event),
    /// Poll error (gateway unreachable, etc.).
    #[allow(dead_code)]
    PollError(String),
}

/// Launch the TUI dashboard.
///
/// Sets up the terminal, runs the async event loop, and restores the terminal
/// on exit or error.
pub async fn run_dashboard(cfg: TuiConfig, team: String) -> anyhow::Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let result = run_loop(&mut terminal, cfg, team).await;

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    result
}

async fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    cfg: TuiConfig,
    team: String,
) -> anyhow::Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let poll_interval = cfg.poll_interval;
    let client = GatewayClient::new(cfg.gateway_url.clone(), cfg.auth_token.clone());
    let compose_dir = cfg.compose_dir.clone();

    let (tx, mut rx) = mpsc::channel::<Tick>(32);

    // Background data poller — polls gateway + reads compose manifests.
    let tx_data = tx.clone();
    let team_clone = team.clone();
    tokio::spawn(async move {
        let mut ticker = interval(poll_interval);
        loop {
            ticker.tick().await;
            let connected = client.healthz().await.unwrap_or(false);
            let tasks_raw = if connected {
                client.list_tasks(&team_clone).await.unwrap_or_default()
            } else {
                vec![]
            };
            let compositions = load_compositions(&compose_dir);
            let tasks = tasks_from_wire(tasks_raw);
            let _ = tx_data
                .send(Tick::Data {
                    connected,
                    compositions,
                    tasks,
                })
                .await;
        }
    });

    // Crossterm input reader.
    let tx_input = tx.clone();
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(ev) = event::read() {
                    let _ = tx_input.send(Tick::Input(ev)).await;
                }
            }
        }
    });

    let mut app = App::new(cfg);
    let mut show_help = false;

    loop {
        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(1)])
                .split(area);

            crate::components::dashboard::draw_dashboard(f, chunks[0], &app);
            crate::statusbar::draw_statusbar(f, chunks[1], &app);

            if show_help {
                let popup_area = centered_rect(60, 70, area);
                crate::help::draw_help(f, popup_area);
            }
        })?;

        while let Ok(tick) = rx.try_recv() {
            match tick {
                Tick::Data {
                    connected,
                    compositions,
                    tasks,
                } => {
                    app.connected = connected;
                    app.compositions = compositions;
                    app.tasks = tasks;
                }
                Tick::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(()),
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        show_help = !show_help;
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {}
                    KeyCode::Esc => {
                        show_help = false;
                    }
                    _ => {}
                },
                Tick::Input(_) => {}
                Tick::PollError(_) => {}
            }
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
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
        .split(vert[1])[1]
}
