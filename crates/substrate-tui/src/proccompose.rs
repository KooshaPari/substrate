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
    pub(crate) prompt_preview: String,
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
    /// TCP port the service listens on (derived from the compose manifest).
    pub port: Option<u16>,
}

impl Composition {
    /// Human-readable uptime string.
    pub fn formatted_uptime(&self) -> String {
        format_duration(self.uptime)
    }
}

// ── Manifest format (JSON on disk) ──────────────────────────────────────

/// Readiness / health probe for a compose service.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadinessProbe {
    /// Shell command or HTTP URL used to check readiness.
    pub command: String,
}

/// Typed representation of a single process-compose JSON config file.
///
/// Use [`load_config`] to parse one file, or [`load_compositions`] to scan a
/// whole directory.
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessComposeConfig {
    /// Service name (must be unique within a compose dir).
    pub name: String,
    /// Path to the compiled binary (relative to workspace root).
    #[serde(default)]
    pub binary: Option<String>,
    /// Command used to start the service (e.g. `cargo run -p foo`).
    #[serde(default)]
    pub command: Option<String>,
    /// Working directory override; `None` means inherit from the launcher.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Optional readiness probe derived from the `health_check` string.
    #[serde(default, deserialize_with = "deserialize_probe")]
    pub readiness_probe: Option<ReadinessProbe>,
    /// TCP port the service listens on, if applicable.
    #[serde(default)]
    pub port: Option<u16>,
    /// Restart policy (`always`, `unless-stopped`, `on-failure`, …).
    #[serde(default)]
    pub restart: Option<String>,
    /// Services that must start before this one.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// Deserialise a plain `health_check` string into a [`ReadinessProbe`].
fn deserialize_probe<'de, D>(de: D) -> Result<Option<ReadinessProbe>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(de)?;
    Ok(opt.map(|command| ReadinessProbe { command }))
}

// Alias kept for internal use so the existing `load_compositions` logic can
// reference the same struct without a separate private type.
type ComposeManifest = ProcessComposeConfig;

// ── Single-file loader ───────────────────────────────────────────────────

/// Parse a single compose JSON file into a [`ProcessComposeConfig`].
///
/// # Errors
/// Returns an error if the file cannot be read or if the JSON is malformed.
pub fn load_config(path: &Path) -> anyhow::Result<ProcessComposeConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    // The on-disk format uses `run_command`; normalise it to `command`.
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("command") {
            if let Some(rc) = obj.remove("run_command") {
                obj.insert("command".into(), rc);
            }
        }
        // Expose `health_check` as `readiness_probe` expected by the struct.
        if !obj.contains_key("readiness_probe") {
            if let Some(hc) = obj.remove("health_check") {
                obj.insert("readiness_probe".into(), hc);
            }
        }
    }
    let cfg: ProcessComposeConfig = serde_json::from_value(value)
        .map_err(|e| anyhow::anyhow!("deserialising {}: {e}", path.display()))?;
    Ok(cfg)
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
        let manifest: ComposeManifest = match load_config(&path) {
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
            model: manifest.command.clone().unwrap_or_default(),
            uptime: Duration::ZERO,
            prompt_preview: String::new(),
        }];

        comps.push(Composition {
            name: manifest.name,
            status,
            members,
            uptime: Duration::ZERO,
            port: manifest.port,
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
