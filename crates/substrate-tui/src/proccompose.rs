//! Proc-compose composition monitoring for the TUI dashboard.
//!
//! Reads JSON compose manifests from a configured directory and builds
//! [`Composition`] and [`Member`] structs for display.

use std::path::Path;
use std::time::Duration;

use ratatui::style::Color;
use serde::Deserialize;
use uuid::Uuid;

// ── Status ──────────────────────────────────────────────────────────────

/// Operational status of a composition.
// Degraded and Error are loaded from proc-compose runtime state; currently
// derive_status produces Running/Stopped only but the variants are kept for
// when live proc-compose HTTP API polling lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionStatus {
    Running,
    Degraded,
    Stopped,
    Error,
}

impl std::fmt::Display for CompositionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "Running"),
            Self::Degraded => write!(f, "Degraded"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Error => write!(f, "Error"),
        }
    }
}

impl CompositionStatus {
    /// Ratatui colour for the status badge.
    // Used in the compositions footer gauge (will be wired to the table title in a follow-up).
    #[allow(dead_code)]
    pub fn state_style(&self) -> Color {
        match self {
            Self::Running => Color::Green,
            Self::Degraded => Color::Yellow,
            Self::Stopped | Self::Error => Color::Red,
        }
    }
}

// ── Member (lane) ───────────────────────────────────────────────────────

/// A single member (dispatch lane) within a composition.
#[derive(Debug, Clone)]
pub struct Member {
    pub id: Uuid,
    pub state: String,
    pub engine: String,
    pub model: String,
    pub uptime: Duration,
    pub prompt_preview: String,
}

impl Member {
    /// Colour derived from the member's current state string.
    pub fn state_style(&self) -> Color {
        match self.state.to_lowercase().as_str() {
            "running" | "working" => Color::Green,
            "degraded" | "throttled" => Color::Yellow,
            "stopped" | "idle" => Color::DarkGray,
            _ => Color::Red,
        }
    }

    /// First 8 hex characters of the member UUID.
    pub fn short_id(&self) -> String {
        let hex = self.id.simple().to_string();
        hex[..8.min(hex.len())].to_owned()
    }

    /// Human-readable uptime string.
    pub fn formatted_uptime(&self) -> String {
        format_duration(self.uptime)
    }

    /// Truncated prompt preview (first 80 chars).
    // Used for display; retained for the detailed member view in a follow-up PR.
    #[allow(dead_code)]
    pub fn prompt_preview(&self) -> &str {
        &self.prompt_preview
    }
}

// ── Composition ─────────────────────────────────────────────────────────

/// A proc-compose group of related members.
#[derive(Debug, Clone)]
pub struct Composition {
    pub name: String,
    pub status: CompositionStatus,
    pub members: Vec<Member>,
    pub uptime: Duration,
}

impl Composition {
    /// Human-readable uptime string.
    pub fn formatted_uptime(&self) -> String {
        format_duration(self.uptime)
    }
}

// ── Manifest format (JSON on disk) ──────────────────────────────────────

/// Raw compose-manifest JSON file format.
// health_check, port, depends_on are parsed for round-trip fidelity and will
// surface in a future proc-compose live-status panel.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ComposeManifest {
    name: String,
    #[serde(default)]
    binary: Option<String>,
    #[serde(default)]
    run_command: Option<String>,
    #[serde(default)]
    health_check: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    restart: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
}

// ── Loading ─────────────────────────────────────────────────────────────

/// Read all JSON files from `compose_dir` and build compositions.
pub fn load_compositions(compose_dir: &Path) -> Vec<Composition> {
    let dir = match std::fs::read_dir(compose_dir) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let mut comps = Vec::new();
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let manifest: ComposeManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let status = derive_status(&manifest.restart);
        let members = vec![Member {
            id: Uuid::new_v4(),
            state: match status {
                CompositionStatus::Running => "running".into(),
                CompositionStatus::Stopped => "stopped".into(),
                _ => "unknown".into(),
            },
            engine: manifest.binary.clone().unwrap_or_default(),
            model: manifest.run_command.clone().unwrap_or_default(),
            uptime: Duration::ZERO,
            prompt_preview: String::new(),
        }];

        comps.push(Composition {
            name: manifest.name,
            status,
            members,
            uptime: Duration::ZERO,
        });
    }
    comps
}

fn derive_status(restart: &Option<String>) -> CompositionStatus {
    match restart.as_deref() {
        Some("always") | Some("unless-stopped") => CompositionStatus::Running,
        Some(_) => CompositionStatus::Stopped,
        None => CompositionStatus::Stopped,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Format a [`Duration`] as a short human string (e.g. "2h 15m").
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_display() {
        assert_eq!(CompositionStatus::Running.to_string(), "Running");
        assert_eq!(CompositionStatus::Degraded.to_string(), "Degraded");
        assert_eq!(CompositionStatus::Stopped.to_string(), "Stopped");
        assert_eq!(CompositionStatus::Error.to_string(), "Error");
    }

    #[test]
    fn status_state_style() {
        assert_eq!(CompositionStatus::Running.state_style(), Color::Green);
        assert_eq!(CompositionStatus::Degraded.state_style(), Color::Yellow);
        assert_eq!(CompositionStatus::Stopped.state_style(), Color::Red);
        assert_eq!(CompositionStatus::Error.state_style(), Color::Red);
    }

    #[test]
    fn member_short_id_truncates() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let m = Member {
            id,
            state: "running".into(),
            engine: "forge".into(),
            model: "gpt-4".into(),
            uptime: Duration::from_secs(3661),
            prompt_preview: "Hello".into(),
        };
        assert_eq!(m.short_id(), "550e8400");
    }

    #[test]
    fn member_formatted_uptime() {
        let id = Uuid::new_v4();
        let m = Member {
            id,
            state: "running".into(),
            engine: "forge".into(),
            model: "gpt-4".into(),
            uptime: Duration::from_secs(7500),
            prompt_preview: String::new(),
        };
        assert_eq!(m.formatted_uptime(), "2h 5m");
    }

    #[test]
    fn member_state_style() {
        let id = Uuid::new_v4();
        let running = Member {
            id,
            state: "running".into(),
            engine: String::new(),
            model: String::new(),
            uptime: Duration::ZERO,
            prompt_preview: String::new(),
        };
        assert_eq!(running.state_style(), Color::Green);
    }
}
