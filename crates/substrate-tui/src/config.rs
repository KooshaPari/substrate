//! Configuration for the substrate TUI dashboard.
//!
//! Read from CLI args / env vars with sensible defaults for local dev.

use std::path::PathBuf;
use std::time::Duration;

/// Dashboard configuration.
#[derive(Clone, Debug)]
pub struct TuiConfig {
    /// Base URL of the gateway (e.g. `http://127.0.0.1:8010`).
    pub gateway_url: String,
    /// Optional bearer token for authenticated routes.
    pub auth_token: Option<String>,
    /// Poll interval for refreshing dispatch state.
    pub poll_interval: Duration,
    /// Proc-compose manifest directory.
    pub compose_dir: PathBuf,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            gateway_url: std::env::var("SUBSTRATE_GATEWAY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8010".into()),
            auth_token: std::env::var("SUBSTRATE_AUTH_TOKEN").ok().filter(|s| !s.is_empty()),
            poll_interval: Duration::from_secs(
                std::env::var("SUBSTRATE_TUI_POLL_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2),
            ),
            compose_dir: std::env::var("SUBSTRATE_COMPOSE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./compose")),
        }
    }
}

impl TuiConfig {
    /// Parse `args` override defaults. Currently only positional gateway URL.
    pub fn from_args() -> Self {
        let mut cfg = Self::default();
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            cfg.gateway_url = args[1].clone();
        }
        cfg
    }
}
