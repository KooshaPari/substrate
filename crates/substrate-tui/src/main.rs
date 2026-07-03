//! Substrate TUI binary — live terminal dashboard for the dispatch/route/plan surface.
//!
//! Launch directly:  `cargo run -p substrate-tui -- --gateway http://127.0.0.1:8010`
//! Or via substrate: `cargo run -p driver-cli -- dash`

// Modules are re-exported via lib.rs; the binary only wires CLI → runner.
use std::time::Duration;

use clap::Parser;
use substrate_tui::config::TuiConfig;
use substrate_tui::runner::run_dashboard;

#[derive(Parser)]
#[command(
    name = "substrate-tui",
    version,
    about = "Live TUI dashboard for the substrate dispatch/route/plan surface.",
    after_help = "ENV:\n  SUBSTRATE_GATEWAY_URL   — gateway base URL (default: http://127.0.0.1:8010)\n  SUBSTRATE_AUTH_TOKEN    — bearer token (optional)\n  SUBSTRATE_TUI_POLL_SECS — poll interval in seconds (default: 2)\n  SUBSTRATE_COMPOSE_DIR   — compose manifest directory (default: ./compose)"
)]
struct Cli {
    /// Gateway base URL (overrides $SUBSTRATE_GATEWAY_URL).
    #[arg(long, value_name = "URL")]
    gateway: Option<String>,

    /// Poll interval in seconds.
    #[arg(long, value_name = "SECS")]
    poll: Option<u64>,

    /// Team name for A2A task queries.
    #[arg(long, value_name = "TEAM", default_value = "")]
    team: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut cfg = TuiConfig::default();
    if let Some(url) = cli.gateway {
        cfg.gateway_url = url;
    }
    if let Some(secs) = cli.poll {
        cfg.poll_interval = Duration::from_secs(secs);
    }
    run_dashboard(cfg, cli.team).await
}
