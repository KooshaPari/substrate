//! Orchestrator-level error type. Never silently swallows; every variant
//! includes actionable context for the caller (per global
//! "Fail clearly, not silently" mandate).

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("failed to read wave.toml at {path}: {source}")]
    WaveIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid TOML in wave.toml at {path}: {message}")]
    WaveParse { path: PathBuf, message: String },

    #[error("invalid wave.toml shape at {path}: {message}")]
    WaveSchema { path: PathBuf, message: String },

    #[error("claude stream-json parse error at byte {offset}: {message}")]
    ClaudeStream { offset: usize, message: String },

    #[error("claude integration is gated (set CLAUDE_INTEGRATION=1 to enable)")]
    ClaudeIntegrationGated,

    #[error("watcher source error for {path}: {message}")]
    Watcher { path: PathBuf, message: String },

    #[error("dispatcher error for task {task}: {message}")]
    Dispatch { task: String, message: String },
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;
