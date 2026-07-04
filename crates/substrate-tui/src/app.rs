//! Application state — top-level struct shared by all TUI components.

use std::time::Instant;

use crate::config::TuiConfig;
use crate::proccompose::{load_compositions, Composition};

/// Top-level application state.
pub struct App {
    /// Dashboard configuration (gateway URL, compose dir, etc.).
    pub config: TuiConfig,
    /// Whether the gateway is currently reachable.
    pub connected: bool,
    /// Live compositions loaded from the compose directory.
    pub compositions: Vec<Composition>,
    /// A2A tasks tracked by the gateway.
    pub tasks: Vec<Task>,
    /// Instant the dashboard started (for uptime calculation).
    startup: Instant,
}

impl App {
    /// Create a new app state from the given configuration.
    ///
    /// Compositions are loaded eagerly from `config.compose_dir`; if the
    /// directory is missing or empty the list is simply empty (no panic).
    pub fn new(config: TuiConfig) -> Self {
        let compositions = load_compositions(&config.compose_dir);
        Self {
            config,
            connected: false,
            compositions,
            tasks: Vec::new(),
            startup: Instant::now(),
        }
    }

    /// Reload compositions from the configured compose directory.
    ///
    /// Call this on a poll tick to pick up changes to the compose manifests.
    pub fn refresh_compositions(&mut self) {
        self.compositions = load_compositions(&self.config.compose_dir);
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
}

/// A tracked A2A task.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: String,
}
