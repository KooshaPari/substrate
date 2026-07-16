//! TOML wave.toml loader.
//!
//! Schema (v1):
//!
//! ```toml
//! name = "sharecli-mvp"
//! dispatcher = "Forge"   # Forge | Codex | Inline (default Forge per MVP path)
//! parallelism = 4
//! timeout_seconds = 600
//!
//! [[tasks]]
//! alias = "sharecli-cli"
//! module = "sharecli::cli"
//! prompt_template = "ship a CLI entry-point for the module"
//! parallelism = 1
//!
//! [[tasks.expectations]]
//! kind = "FileExists"
//! value = "src/bin/foo.rs"
//!
//! [[tasks.expectations]]
//! kind = "FunctionExists"
//! value = "sharecli::cli::run"
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{OrchestratorError, Result};

/// Default dispatcher per MVP-path memory cut-line.
pub const DEFAULT_DISPATCHER: DispatcherKind = DispatcherKind::Forge;

/// What kind of agent runner should handle a wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DispatcherKind {
    /// `forge -p "$(cat wave.toml-context)"` — default per MVP-path spec.
    Forge,
    /// `codex exec "<prompt>"` (the prompt is positional).
    Codex,
    /// In-process `Inline` fallback (mirrors the WaveRunner default).
    Inline,
}

impl Default for DispatcherKind {
    fn default() -> Self {
        DEFAULT_DISPATCHER
    }
}

/// Top-level wave definition loaded from a `wave.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveConfig {
    /// Human-readable wave label; surfaces in `WaveReport` and audit logs.
    pub name: String,
    /// Which dispatcher to use. Defaults to `Forge` when missing.
    #[serde(default)]
    pub dispatcher: DispatcherKind,
    /// Tasks to execute in parallel.
    #[serde(default)]
    pub tasks: Vec<TaskSpec>,
    /// Maximum simultaneous tasks. `0` ⇒ treat as `tasks.len()`.
    #[serde(default)]
    pub parallelism: u32,
    /// Per-task timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_timeout() -> u64 {
    600
}

impl Default for WaveConfig {
    fn default() -> Self {
        Self {
            name: String::from("untitled-wave"),
            dispatcher: DispatcherKind::default(),
            tasks: Vec::new(),
            parallelism: 0,
            timeout_seconds: default_timeout(),
        }
    }
}

/// A single task inside a `WaveConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Short alias used in `WaveReport::failed`.
    pub alias: String,
    /// Module reference (e.g. `sharecli::cli`, `substrate::orchestrator`).
    pub module: String,
    /// Prompt template; orchestrator renders and hands to dispatcher.
    pub prompt_template: String,
    /// Expectations checked at completion (best-effort lint).
    #[serde(default)]
    pub expectations: Vec<Expectation>,
    /// Per-task parallelism override; `0` ⇒ wave-level.
    #[serde(default)]
    pub parallelism: u32,
}

/// A single pass/fail check applied to a completed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expectation {
    pub kind: ExpectationKind,
    pub value: String,
}

/// Supported expectation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ExpectationKind {
    /// A file at the given workspace-relative path must exist.
    FileExists,
    /// A fully-qualified function path must resolve.
    FunctionExists,
    /// `cargo test`/`vitest` containing the given substring must pass.
    TestPasses,
    /// A specific CSS hex token must appear in a checked file.
    CssProperty,
}

/// Load and validate a `wave.toml` from disk.
pub fn load_wave(path: &Path) -> Result<WaveConfig> {
    let text = std::fs::read_to_string(path).map_err(|source| OrchestratorError::WaveIo {
        path: path.to_path_buf(),
        source,
    })?;

    let cfg: WaveConfig = toml::from_str(&text).map_err(|e| OrchestratorError::WaveParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    validate(&cfg, path)?;
    Ok(cfg)
}

/// Light semantic validation; runs after parsing succeeds.
fn validate(cfg: &WaveConfig, path: &Path) -> Result<()> {
    if cfg.name.trim().is_empty() {
        return Err(OrchestratorError::WaveSchema {
            path: path.to_path_buf(),
            message: "`name` must be non-empty".into(),
        });
    }
    if cfg.tasks.is_empty() {
        return Err(OrchestratorError::WaveSchema {
            path: path.to_path_buf(),
            message: "at least one [[tasks]] entry required".into(),
        });
    }
    let mut seen = std::collections::HashSet::new();
    for task in &cfg.tasks {
        if task.alias.trim().is_empty() {
            return Err(OrchestratorError::WaveSchema {
                path: path.to_path_buf(),
                message: "task.alias must be non-empty".into(),
            });
        }
        if !seen.insert(task.alias.as_str()) {
            return Err(OrchestratorError::WaveSchema {
                path: path.to_path_buf(),
                message: format!("duplicate task alias: {}", task.alias),
            });
        }
        if task.prompt_template.trim().is_empty() {
            return Err(OrchestratorError::WaveSchema {
                path: path.to_path_buf(),
                message: format!("task.{}: prompt_template must be non-empty", task.alias),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_wave(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("tmp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn parses_all_fields() {
        let body = r##"
            name = "sharecli-mvp"
            dispatcher = "Forge"
            parallelism = 4
            timeout_seconds = 900

            [[tasks]]
            alias = "sharecli-cli"
            module = "sharecli::cli"
            prompt_template = "ship the CLI entry"
            parallelism = 1

            [[tasks.expectations]]
            kind = "FileExists"
            value = "src/bin/sharecli.rs"

            [[tasks.expectations]]
            kind = "FunctionExists"
            value = "sharecli::cli::run"

            [[tasks.expectations]]
            kind = "TestPasses"
            value = "cli_smoke"

            [[tasks.expectations]]
            kind = "CssProperty"
            value = "#3fb950"
        "##;
        let f = tmp_wave(body);
        let cfg = load_wave(f.path()).expect("parse");
        assert_eq!(cfg.name, "sharecli-mvp");
        assert_eq!(cfg.dispatcher, DispatcherKind::Forge);
        assert_eq!(cfg.parallelism, 4);
        assert_eq!(cfg.timeout_seconds, 900);
        assert_eq!(cfg.tasks.len(), 1);
        let t = &cfg.tasks[0];
        assert_eq!(t.alias, "sharecli-cli");
        assert_eq!(t.expectations.len(), 4);
        assert_eq!(t.expectations[3].kind, ExpectationKind::CssProperty);
        assert_eq!(t.expectations[3].value, "#3fb950");
    }

    #[test]
    fn defaults_dispatcher_to_forge() {
        let body = r#"
            name = "x"
            [[tasks]]
            alias = "t"
            module = "m"
            prompt_template = "p"
        "#;
        let f = tmp_wave(body);
        let cfg = load_wave(f.path()).expect("parse");
        assert_eq!(cfg.dispatcher, DispatcherKind::Forge);
        assert_eq!(cfg.timeout_seconds, 600);
    }

    #[test]
    fn rejects_empty_name() {
        let body = r#"
            name = ""
            [[tasks]]
            alias = "t"
            module = "m"
            prompt_template = "p"
        "#;
        let f = tmp_wave(body);
        assert!(load_wave(f.path()).is_err());
    }

    #[test]
    fn rejects_duplicate_aliases() {
        let body = r#"
            name = "x"
            [[tasks]]
            alias = "dup"
            module = "m1"
            prompt_template = "p1"
            [[tasks]]
            alias = "dup"
            module = "m2"
            prompt_template = "p2"
        "#;
        let f = tmp_wave(body);
        let err = load_wave(f.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate task alias"));
    }

    #[test]
    fn rejects_empty_prompt_template() {
        let body = r#"
            name = "x"
            [[tasks]]
            alias = "t"
            module = "m"
            prompt_template = "   "
        "#;
        let f = tmp_wave(body);
        assert!(load_wave(f.path()).is_err());
    }
}
