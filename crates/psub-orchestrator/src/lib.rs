//! L120 MVP orchestrator
//!
//! Cut-line (per MVP-path memory):
//!   TOML wave.toml loader → existing WaveRunner (default dispatcher = forge),
//!   claude -p stream-json parser gated by CLAUDE_INTEGRATION=1,
//!   JSONL `~/.claude/projects/.../tasks/*.jsonl` → WatcherPort::ToolCall.
//!
//! Sub-modules:
//!   * `wave`        — TOML loader and types (WaveConfig, TaskSpec, Expectation, DispatcherKind)
//!   * `dispatcher`  — `Dispatcher` trait and a built-in `ForgeDispatcher` stub
//!   * `claude_stream` — claude -p `--output-format stream-json` parser (gated)
//!   * `watcher`     — JSONL Tailer that adapts task records into `ToolCall`s
//!   * `runner`      — `run_wave()` async driver emitting `WaveReport`
//!
//! Zero network calls on first import; activation gated by feature/env so the
//! crate can sit in the workspace without forcing operator-side dependencies.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod claude_stream;
pub mod dispatcher;
pub mod error;
pub mod runner;
pub mod stream;
pub mod watcher;
pub mod wave;

pub use error::{OrchestratorError, Result};
pub use runner::{run_wave, DispatchOutcome, Dispatcher, FailedTask, TaskHandle, WaveReport};
pub use wave::{
    DispatcherKind, Expectation, ExpectationKind, TaskSpec, WaveConfig, load_wave,
};
pub use watcher::{ToolCall, WatcherSource, watch_project_tasks};
pub use claude_stream::{ClaudeEvent, claude_stream_available, parse_claude_stream_json};
