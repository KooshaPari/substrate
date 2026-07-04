
//! Substrate TUI — terminal UI dashboard for the substrate dispatch surface.

mod boot;
mod sparkline;
mod app;
mod components;
mod config;
mod dispatch_client;
mod help;
mod proccompose;
mod statusbar;

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use config::TuiConfig;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const EVENT_TIMEOUT: Duration = Duration::from_millis(100);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── terminal setup ──────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app; ensure we restore the terminal even on error.
    let result = run_app(&mut terminal).await;

    // ── terminal teardown ───────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    let config = TuiConfig::from_args();
    let mut app = App::new(config);

    // Kick off an initial status refresh, metrics fetch, and log fetch.
    app.refresh_service_statuses().await;
    app.refresh_metrics().await;
    app.refresh_logs().await;

    let mut last_refresh = Instant::now();

    loop {
        // ── render ──────────────────────────────────────────────────────────
        terminal.draw(|f| app.render(f))?;

        // ── poll for crossterm events (100 ms timeout) ───────────────────────
        if event::poll(EVENT_TIMEOUT)? {
            if let Event::Key(key) = event::read()? {
                // If help is open, any key closes it first.
                if app.show_help {
                    app.toggle_help();
                    continue;
                }

                match (key.code, key.modifiers) {
                    // Quit
                    (KeyCode::Char('q'), _)
                    | (KeyCode::Char('Q'), _)
                    | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,

                    // Force refresh
                    (KeyCode::Char('r'), _) | (KeyCode::Char('R'), _) => {
                        app.refresh_service_statuses().await;
                        last_refresh = Instant::now();
                    }

                    // Navigate down
                    (KeyCode::Char('j'), _) | (KeyCode::Down, _) => app.select_next(),

                    // Navigate up
                    (KeyCode::Char('k'), _) | (KeyCode::Up, _) => app.select_prev(),

                    // Toggle metrics panel
                    (KeyCode::Char('m'), _) | (KeyCode::Char('M'), _) => app.toggle_metrics(),

                    // Toggle request log panel
                    (KeyCode::Char('l'), _) | (KeyCode::Char('L'), _) => {
                        app.toggle_logs();
                        if app.show_logs {
                            app.refresh_logs().await;
                        }
                    }

                    // Toggle help
                    (KeyCode::Char('?'), _) | (KeyCode::Char('h'), _) => app.toggle_help(),

                    // Enter — currently a no-op placeholder for detail view
                    (KeyCode::Enter, _) => {}

                    _ => {}
                }
            }
        }

        // ── poll timer: refresh every 5 s ────────────────────────────────────
        if last_refresh.elapsed() >= POLL_INTERVAL {
            app.refresh_service_statuses().await;
            app.refresh_metrics().await;
            app.refresh_logs().await;
            last_refresh = Instant::now();
        }
    }

    Ok(())
}
