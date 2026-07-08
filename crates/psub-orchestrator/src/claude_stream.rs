//! `claude -p --output-format stream-json` parser.
//!
//! Activation is gated by the `CLAUDE_INTEGRATION=1` env var (per MVP-path spec).
//! When unset, [`parse_claude_stream_json`] returns
//! [`OrchestratorError::ClaudeIntegrationGated`] so callers fail loudly instead
//! of silently streaming nothing.

use std::io::Read;

use futures_core::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{OrchestratorError, Result};

/// One event emitted on the `stream-json` protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeEvent {
    /// Incremental assistant text delta.
    AssistantDelta {
        /// Concatenated text accumulated for this turn at the time of the event.
        text: String,
    },
    /// A tool use request issued by claude.
    ToolUse {
        /// Stable id used to correlate with [`ClaudeEvent::ToolResult`].
        id: String,
        /// Tool name (e.g. `Bash`, `Read`, `Write`).
        name: String,
        /// Free-form input args for the tool.
        input: Value,
    },
    /// Final result of a single tool invocation.
    ToolResult {
        /// Matching tool use id.
        tool_use_id: String,
        /// Output value.
        output: Value,
        /// Whether the tool call errored.
        is_error: bool,
    },
    /// Whole-run summary line printed at the end of `claude -p`.
    Result {
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
        /// Cost incurred in USD for this invocation.
        cost_usd: f64,
    },
    /// Structured error envelope from the CLI.
    Error {
        /// Numeric exit-like code.
        code: i32,
        /// Human-readable error message.
        message: String,
    },
}

/// Returns `true` when the operator has opted into the claude integration.
///
/// Checked by env var (not feature flag) so ops can flip it on a deployed
/// binary without recompiling.
pub fn claude_stream_available() -> bool {
    std::env::var("CLAUDE_INTEGRATION")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false)
}

/// Parse a `claude -p --output-format stream-json` byte stream into a
/// `Stream<Item = Result<ClaudeEvent>>`.
///
/// The reader is consumed eagerly into a buffered event vec; the resulting
/// `Stream` is yielded via `crate::stream::EventStream`.
///
/// Returns [`OrchestratorError::ClaudeIntegrationGated`] when the gate is
/// closed, [`OrchestratorError::Watcher`] on read failure.
pub fn parse_claude_stream_json<R: Read>(
    mut reader: R,
) -> Result<impl Stream<Item = Result<ClaudeEvent>>> {
    if !claude_stream_available() {
        return Err(OrchestratorError::ClaudeIntegrationGated);
    }
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .map_err(|e| OrchestratorError::Watcher {
            path: std::path::PathBuf::from("<stream-json>"),
            message: format!("read failed: {e}"),
        })?;
    let events = parse_claude_stream_buffer(&buf)?;
    Ok(crate::stream::EventStream::new(events))
}

/// Parse the entire buffer at once. Public so JSONL watchers (e.g. tails of
/// `~/.claude/projects/.../tasks/*.jsonl`) can reuse the same decoder.
pub fn parse_claude_stream_buffer(buf: &str) -> Result<Vec<Result<ClaudeEvent>>> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for (line_no, raw) in buf.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            offset += raw.len() + 1;
            continue;
        }
        match serde_json::from_str::<ClaudeEvent>(trimmed) {
            Ok(ev) => out.push(Ok(ev)),
            Err(e) => out.push(Err(OrchestratorError::ClaudeStream {
                offset: offset + (raw.len() - trimmed.len()),
                message: format!("line {}: {e}", line_no + 1),
            })),
        }
        offset += raw.len() + 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWaker;
    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }

    fn drain<S: Stream + Unpin>(mut s: S) -> Vec<S::Item> {
        let waker = Waker::from(Arc::new(NoopWaker));
        let mut cx = Context::from_waker(&waker);
        let mut out = Vec::new();
        loop {
            match <S as Stream>::poll_next(Pin::new(&mut s), &mut cx) {
                Poll::Ready(Some(item)) => out.push(item),
                Poll::Ready(None) => return out,
                Poll::Pending => continue,
            }
        }
    }

    fn enable_gate() {
        std::env::set_var("CLAUDE_INTEGRATION", "1");
    }
    fn disable_gate() {
        std::env::remove_var("CLAUDE_INTEGRATION");
    }

    #[test]
    fn parses_three_real_events() {
        enable_gate();
        let body = r#"{"type":"assistant_delta","text":"hello"}
{"type":"tool_use","id":"t1","name":"Read","input":{"path":"./README.md"}}
{"type":"result","duration_ms":1234,"cost_usd":0.0023}
"#;
        let mut cursor = Cursor::new(body.as_bytes());
        let parsed = parse_claude_stream_json(&mut cursor).expect("stream");
        let v = drain(parsed);
        disable_gate();

        assert_eq!(v.len(), 3);
        match &v[0] {
            Ok(ClaudeEvent::AssistantDelta { text }) => assert_eq!(text, "hello"),
            other => panic!("unexpected 0: {other:?}"),
        }
        match &v[1] {
            Ok(ClaudeEvent::ToolUse { id, name, input }) => {
                assert_eq!(id, "t1");
                assert_eq!(name, "Read");
                assert_eq!(input["path"], "./README.md");
            }
            other => panic!("unexpected 1: {other:?}"),
        }
        match &v[2] {
            Ok(ClaudeEvent::Result { duration_ms, cost_usd }) => {
                assert_eq!(*duration_ms, 1234);
                assert!((cost_usd - 0.0023).abs() < 1e-9);
            }
            other => panic!("unexpected 2: {other:?}"),
        }
    }

    #[test]
    fn gated_when_env_unset() {
        disable_gate();
        let body = r#"{"type":"result","duration_ms":1,"cost_usd":0.0}"#;
        let mut cursor = Cursor::new(body.as_bytes());
        let err = parse_claude_stream_json(&mut cursor).unwrap_err();
        assert!(matches!(err, OrchestratorError::ClaudeIntegrationGated));
    }

    #[test]
    fn reports_malformed_line() {
        enable_gate();
        let body = r#"{"type":"result","duration_ms":1,"cost_usd":0.0}
{"type":"tool_result","tool_use_id":"t1","output":"ok","is_error":false}
this is not json
"#;
        let mut cursor = Cursor::new(body.as_bytes());
        let stream = parse_claude_stream_json(&mut cursor).expect("stream");
        let v = drain(stream);
        disable_gate();

        assert_eq!(v.len(), 3);
        assert!(matches!(v[0], Ok(ClaudeEvent::Result { .. })));
        assert!(matches!(v[1], Ok(ClaudeEvent::ToolResult { .. })));
        assert!(matches!(v[2], Err(OrchestratorError::ClaudeStream { .. })));
    }
}
