//! Configuration for thegent-dispatch.
//!
//! Configuration is loaded from the following sources (each layer overrides
//! the previous):
//!
//! 1. Built-in defaults
//! 2. `./thegent-dispatch.toml` (local per-directory config)
//! 3. `$XDG_CONFIG_HOME/thegent-dispatch/config.toml`
//! 4. Environment variables prefixed with `THGENT_`
//!
//! # Environment variable mapping
//!
//! Each field maps to `THGENT_<UPPERCASED_FIELD_NAME>`. For example:
//!
//! | Field               | Env var                        |
//! |---------------------|--------------------------------|
//! | `provider_forge`    | `THGENT_PROVIDER_FORGE`        |
//! | `default_timeout_s` | `THGENT_DEFAULT_TIMEOUT_S`     |
//! | `tracing_level`     | `THGENT_TRACING_LEVEL`         |

use figment::providers::{Env, Serialized, Toml};
use figment::Figment;
use serde::Deserialize;
use std::path::PathBuf;

/// Global configuration for thegent-dispatch.
///
/// Every field has a sensible default so the tool works out-of-the-box
/// without any configuration file or environment variables.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    // ------------------------------------------------------------------
    // Provider binary paths
    // ------------------------------------------------------------------

    /// Binary name or path for the Forge provider CLI.
    pub provider_forge: String,

    /// Binary name or path for the Codex provider CLI.
    pub provider_codex: String,

    /// Binary name or path for the Gemini provider CLI.
    pub provider_gemini: String,

    /// Subcommand passed to the Gemini binary (e.g. "chat").
    pub provider_gemini_subcommand: String,

    /// Binary name or path for the Copilot provider CLI.
    pub provider_copilot: String,

    /// Binary name or path for the Cursor provider CLI (headless stub).
    pub provider_cursor: String,

    /// Binary name or path for the Droid provider CLI.
    pub provider_droid: String,

    /// Binary name or path for the Minimax provider CLI (via cheap-llm).
    pub provider_minimax: String,

    /// Provider identifier passed to the Minimax / cheap-llm router (e.g. "minimax").
    pub provider_minimax_route: String,

    /// Binary name or path for the Claude provider CLI.
    pub provider_claude: String,

    /// Subcommand passed to the Claude binary (e.g. "chat").
    pub provider_claude_subcommand: String,

    // ------------------------------------------------------------------
    // BG session wrapper
    // ------------------------------------------------------------------

    /// Binary used to wrap background sessions.
    pub bg_wrapper: String,

    /// Output format passed to the bg wrapper (`--format <value>`).
    pub bg_wrapper_format: String,

    // ------------------------------------------------------------------
    // Defaults
    // ------------------------------------------------------------------

    /// Default timeout in seconds when `--timeout-s` is not given on the CLI.
    pub default_timeout_s: u64,

    /// Default working directory when `--cwd` is not given on the CLI.
    pub default_cwd: PathBuf,

    // ------------------------------------------------------------------
    // Logging / tracing
    // ------------------------------------------------------------------

    /// Default tracing level (passed to `EnvFilter`). One of:
    /// `error`, `warn`, `info`, `debug`, `trace`.
    pub tracing_level: String,

    // ------------------------------------------------------------------
    // Display
    // ------------------------------------------------------------------

    /// Title text shown in the dry-run panel.
    pub dry_run_panel_title: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Provider binaries
            provider_forge: "forge".into(),
            provider_codex: "codex-agent".into(),
            provider_gemini: "gemini".into(),
            provider_gemini_subcommand: "chat".into(),
            provider_copilot: "copilot-agent".into(),
            provider_cursor: "echo".into(),
            provider_droid: "run_droid.sh".into(),
            provider_minimax: "cheap-llm".into(),
            provider_minimax_route: "minimax".into(),
            provider_claude: "claude".into(),
            provider_claude_subcommand: "chat".into(),

            // BG session wrapper
            bg_wrapper: "thegent".into(),
            bg_wrapper_format: "json".into(),

            // Defaults
            default_timeout_s: 600,
            default_cwd: PathBuf::from("."),

            // Logging
            tracing_level: "info".into(),

            // Display
            dry_run_panel_title: "thegent-dispatch - dry run".into(),
        }
    }
}

impl Config {
    /// Load configuration from the layered sources.
    ///
    /// # Errors
    ///
    /// Returns `figment::Error` if any provider fails (e.g. malformed TOML).
    pub fn load() -> Result<Self, figment::Error> {
        // Determine XDG config path for thegent-dispatch
        let xdg_config = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("thegent-dispatch")
            .join("config.toml");

        let config: Config = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file("thegent-dispatch.toml"))
            .merge(Toml::file(xdg_config))
            .merge(Env::prefixed("THGENT_"))
            .extract()?;

        Ok(config)
    }

    /// Load configuration, falling back to defaults if no file or env overrides exist.
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_sane() {
        let cfg = Config::default();
        assert_eq!(cfg.provider_forge, "forge");
        assert_eq!(cfg.provider_codex, "codex-agent");
        assert_eq!(cfg.provider_gemini, "gemini");
        assert_eq!(cfg.provider_gemini_subcommand, "chat");
        assert_eq!(cfg.provider_copilot, "copilot-agent");
        assert_eq!(cfg.provider_cursor, "echo");
        assert_eq!(cfg.provider_droid, "run_droid.sh");
        assert_eq!(cfg.provider_minimax, "cheap-llm");
        assert_eq!(cfg.provider_minimax_route, "minimax");
        assert_eq!(cfg.provider_claude, "claude");
        assert_eq!(cfg.provider_claude_subcommand, "chat");
        assert_eq!(cfg.bg_wrapper, "thegent");
        assert_eq!(cfg.bg_wrapper_format, "json");
        assert_eq!(cfg.default_timeout_s, 600);
        assert_eq!(cfg.default_cwd, PathBuf::from("."));
        assert_eq!(cfg.tracing_level, "info");
        assert_eq!(cfg.dry_run_panel_title, "thegent-dispatch - dry run");
    }

    #[test]
    fn config_serde_roundtrip() {
        // Serialize defaults to JSON and deserialize back.
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.provider_forge, cfg.provider_forge);
        assert_eq!(restored.default_timeout_s, cfg.default_timeout_s);
    }
}
