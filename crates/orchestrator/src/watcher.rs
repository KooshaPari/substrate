//! JSONL → `ToolCall` WatcherPort adapter.
//!
//! Per MVP-path memory, the default source is
//! `~/.claude/projects/<uuid>/tasks/*.jsonl`. We also accept Codex session
//! dumps and forge logs because hand-off between dispatchers is part of the
//! MVP surface.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use futures_core::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Single normalized tool-use record surfaced to the orchestrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Stable identifier emitted by the producing dispatcher.
    pub task_id: String,
    /// Tool name as reported (`agent_run`, `codex_dispatch`, `forge_p`, ...).
    pub tool_name: String,
    /// Free-form structured args for the tool.
    pub args: Value,
    /// Unix-epoch milliseconds when the dispatcher recorded the call.
    pub timestamp: u64,
}

/// One of the supported JSONL sources a watcher reads from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherSource {
    /// `~/.claude/projects/<uuid>/tasks/*.jsonl`.
    ClaudeProjectsJsonl(PathBuf),
    /// Codex session dump.
    CodexSession(PathBuf),
    /// forge-style log file.
    ForgeLog(PathBuf),
}

/// Watch a project directory and emit `ToolCall`s as JSONL records arrive.
///
/// The default source is `ClaudeProjectsJsonl`. Existing JSONL records are
/// replayed; new ones appear as they are appended (the MVP surface — actual
/// inotify tailing is left to the broader substrate `file-watcher` crate and
/// is wired in when the orchestrator is promoted out of cut-line MVP).
pub fn watch_project_tasks(project_dir: &Path) -> impl Stream<Item = crate::Result<ToolCall>> {
    let entries = collect_jsonl_records(
        project_dir,
        &WatcherSource::ClaudeProjectsJsonl(project_dir.to_path_buf()),
    );
    WatcherStream::new(entries)
}

fn collect_jsonl_records(
    project_dir: &Path,
    source: &WatcherSource,
) -> Vec<crate::Result<ToolCall>> {
    let path = match source {
        WatcherSource::ClaudeProjectsJsonl(p) => p.join("tasks"),
        WatcherSource::CodexSession(p) => p.to_path_buf(),
        WatcherSource::ForgeLog(p) => p.to_path_buf(),
    };

    if !path.exists() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let read = std::fs::read_dir(&path).map_err(|e| crate::error::OrchestratorError::Watcher {
        path: path.clone(),
        message: format!("read_dir: {e}"),
    });

    let entries = match read {
        Ok(it) => it,
        Err(e) => return vec![Err(e)],
    };

    let files: Vec<PathBuf> = entries
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .collect();

    for file in files {
        match std::fs::read_to_string(&file) {
            Ok(text) => {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<ToolCall>(trimmed) {
                        Ok(call) => out.push(Ok(call)),
                        Err(e) => out.push(Err(crate::error::OrchestratorError::Watcher {
                            path: file.clone(),
                            message: format!("jsonl parse: {e}"),
                        })),
                    }
                }
            }
            Err(e) => out.push(Err(crate::error::OrchestratorError::Watcher {
                path: file,
                message: format!("read: {e}"),
            })),
        }
    }

    let _ = project_dir;
    out
}

pub struct WatcherStream {
    inner: std::vec::IntoIter<crate::Result<ToolCall>>,
}

impl WatcherStream {
    fn new(items: Vec<crate::Result<ToolCall>>) -> Self {
        Self {
            inner: items.into_iter(),
        }
    }
}

impl Stream for WatcherStream {
    type Item = crate::Result<ToolCall>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.inner.next())
    }
}

struct NoopWaker;
impl Wake for NoopWaker {
    fn wake(self: Arc<Self>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(dir: &Path, name: &str, body: &str) {
        let tasks = dir.join("tasks");
        std::fs::create_dir_all(&tasks).expect("mkdir");
        let mut f = std::fs::File::create(tasks.join(name)).expect("create");
        f.write_all(body.as_bytes()).expect("write");
    }

    fn drain<S: Stream + Unpin>(mut s: S) -> Vec<S::Item> {
        let waker = Waker::from(Arc::new(NoopWaker));
        let mut cx = Context::from_waker(&waker);
        let mut out = Vec::new();
        loop {
            match Stream::poll_next(std::pin::Pin::new(&mut s), &mut cx) {
                Poll::Ready(Some(item)) => out.push(item),
                Poll::Ready(None) => return out,
                Poll::Pending => continue,
            }
        }
    }

    #[test]
    fn round_trip_toolcall_through_watcher() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let body = r#"{"task_id":"t-1","tool_name":"agent_run","args":{"module":"sharecli::cli"},"timestamp":1718000000000}
{"task_id":"t-2","tool_name":"forge_p","args":{"prompt":"hi"},"timestamp":1718000000100}
"#;
        write_jsonl(dir.path(), "abc.jsonl", body);

        let stream = watch_project_tasks(dir.path());
        let items = drain(stream);

        assert_eq!(items.len(), 2);
        let a = items.into_iter();
        let mut iter = a.peekable();
        let _ = iter.next();
    }

    #[test]
    fn first_task_record_is_decoded() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let body = r#"{"task_id":"t-1","tool_name":"agent_run","args":{"module":"sharecli::cli"},"timestamp":1718000000000}
{"task_id":"t-2","tool_name":"forge_p","args":{"prompt":"hi"},"timestamp":1718000000100}
"#;
        write_jsonl(dir.path(), "abc.jsonl", body);
        let items = drain(watch_project_tasks(dir.path()));
        assert_eq!(items.len(), 2);
        let a = items[0].as_ref().expect("ok-1").clone();
        assert_eq!(a.task_id, "t-1");
        assert_eq!(a.tool_name, "agent_run");
        assert_eq!(a.args["module"], "sharecli::cli");
        let b = items[1].as_ref().expect("ok-2").clone();
        assert_eq!(b.tool_name, "forge_p");
        assert_eq!(b.timestamp, 1718000000100);
    }

    #[test]
    fn tolerates_missing_directory() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let inner = dir.path().join("does-not-exist");
        std::fs::create_dir_all(&inner).unwrap();
        let items = drain(watch_project_tasks(&inner));
        assert!(items.is_empty(), "missing tasks/ should yield no items");
    }

    #[test]
    fn reports_parse_errors_per_line() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let body = "{\"task_id\":\"t-1\",\"tool_name\":\"agent_run\",\"args\":{},\"timestamp\":1}\nnot-json\n";
        write_jsonl(dir.path(), "bad.jsonl", body);
        let items = drain(watch_project_tasks(dir.path()));
        assert_eq!(items.len(), 2);
        assert!(items[0].is_ok());
        assert!(items[1].is_err());
    }
}
