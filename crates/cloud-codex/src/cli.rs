//! `codex cloud` CLI invocation and output parsing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use substrate_core::cloud_dispatch_port::{CloudResult, CloudTaskHandle, CloudTaskStatus};
use substrate_core::error::{Result, SubstrateError};
use tokio::process::Command;

/// Environment variable for the Codex Cloud environment id (`codex cloud exec --env`).
pub const ENV_CLOUD_ENV_ID: &str = "CODEX_CLOUD_ENV_ID";

/// Configuration for [`CodexCloudDispatch`].
#[derive(Debug, Clone)]
pub struct CodexCloudConfig {
    /// Path to the `codex` binary (default from `CODEX_BIN` or `"codex"`).
    pub bin: String,
    /// Target Codex Cloud environment id.
    pub env_id: String,
}

#[derive(Debug, Clone)]
struct TaskMeta {
    task_id: String,
    branch: String,
    task_url: Option<String>,
    last_summary: Option<String>,
}

/// Output from a `codex` subprocess invocation.
#[derive(Debug, Clone)]
pub struct CodexCommandOutput {
    /// Process exit status.
    pub status: std::process::ExitStatus,
    /// Captured stdout (UTF-8 lossy).
    pub stdout: String,
    /// Captured stderr (UTF-8 lossy).
    pub stderr: String,
}

/// Abstraction over `codex` subprocess execution (real or mocked).
#[async_trait]
pub trait CodexCommandRunner: Send + Sync {
    /// Run `bin` with `args` and capture output.
    async fn run(&self, bin: &str, args: &[String]) -> Result<CodexCommandOutput>;
}

/// Default runner using `tokio::process::Command`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioCodexRunner;

#[async_trait]
impl CodexCommandRunner for TokioCodexRunner {
    async fn run(&self, bin: &str, args: &[String]) -> Result<CodexCommandOutput> {
        let output = Command::new(bin)
            .args(args)
            .output()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("spawn {bin}: {e}")))?;
        Ok(CodexCommandOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Codex Cloud CLI adapter (`codex cloud exec|status|diff|apply`).
#[derive(Clone)]
pub struct CodexCloudDispatch {
    config: CodexCloudConfig,
    runner: Arc<dyn CodexCommandRunner>,
    tasks: Arc<Mutex<HashMap<String, TaskMeta>>>,
}

impl std::fmt::Debug for CodexCloudDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexCloudDispatch")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CodexCloudDispatch {
    /// Build from environment (`CODEX_BIN`, `CODEX_CLOUD_ENV_ID`).
    ///
    /// Auth is provided by the Codex CLI session (`codex login`); no separate
    /// API key env var is required for cloud commands.
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
        let env_id = std::env::var(ENV_CLOUD_ENV_ID).map_err(|e| {
            SubstrateError::CloudDispatch(format!("{ENV_CLOUD_ENV_ID} not set: {e}"))
        })?;
        Ok(Self::new(CodexCloudConfig { bin, env_id }))
    }

    /// Build with explicit config and the default tokio subprocess runner.
    pub fn new(config: CodexCloudConfig) -> Self {
        Self::with_runner(config, Arc::new(TokioCodexRunner))
    }

    /// Build with an injectable runner (tests / scripted fakes).
    pub fn with_runner(config: CodexCloudConfig, runner: Arc<dyn CodexCommandRunner>) -> Self {
        Self {
            config,
            runner,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Submit a cloud task via `codex cloud exec`.
    pub async fn submit(&self, repo: &str, branch: &str, prompt: &str) -> Result<CloudTaskHandle> {
        let _ = repo; // repo is defined by the Codex Cloud environment configuration
        let args = vec![
            "cloud".into(),
            "exec".into(),
            "--env".into(),
            self.config.env_id.clone(),
            "--branch".into(),
            branch.to_string(),
            prompt.to_string(),
        ];
        let output = self.run_cli(&args).await?;
        if !output.status.success() {
            return Err(SubstrateError::CloudDispatch(format!(
                "codex cloud exec failed (exit={}): {}",
                output.status,
                cli_error_text(&output)
            )));
        }

        let task_url = output
            .stdout
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let task_id = parse_task_id_from_output(&task_url).ok_or_else(|| {
            SubstrateError::CloudDispatch(format!(
                "codex cloud exec: could not parse task id from stdout: {task_url:?}"
            ))
        })?;

        let handle_id = format!("codex-{task_id}");
        self.tasks.lock().unwrap().insert(
            handle_id.clone(),
            TaskMeta {
                task_id,
                branch: branch.to_string(),
                task_url: if task_url.is_empty() {
                    None
                } else {
                    Some(task_url)
                },
                last_summary: None,
            },
        );

        Ok(CloudTaskHandle { id: handle_id })
    }

    /// Poll task status via `codex cloud status`.
    pub async fn poll(&self, handle: &CloudTaskHandle) -> Result<CloudTaskStatus> {
        let meta = self
            .tasks
            .lock()
            .unwrap()
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        let args = vec!["cloud".into(), "status".into(), meta.task_id.clone()];
        let output = self.run_cli(&args).await?;

        if let Some(summary) = parse_summary_line(&output.stdout) {
            let mut guard = self.tasks.lock().unwrap();
            if let Some(rec) = guard.get_mut(&handle.id) {
                rec.last_summary = Some(summary);
            }
        }

        if let Some(raw) = parse_status_label(&output.stdout) {
            return Ok(map_codex_status(&raw, cli_error_text(&output)));
        }

        if output.status.success() {
            return Ok(CloudTaskStatus::Succeeded);
        }

        Err(SubstrateError::CloudDispatch(format!(
            "codex cloud status failed (exit={}): {}",
            output.status,
            cli_error_text(&output)
        )))
    }

    /// Harvest diff metadata via `codex cloud diff` (and optional apply preflight).
    pub async fn harvest_task(&self, handle: &CloudTaskHandle) -> Result<CloudResult> {
        let meta = self
            .tasks
            .lock()
            .unwrap()
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        let status_args = vec!["cloud".into(), "status".into(), meta.task_id.clone()];
        let status_out = self.run_cli(&status_args).await?;
        let status = parse_status_label(&status_out.stdout)
            .map(|raw| map_codex_status(&raw, cli_error_text(&status_out)))
            .unwrap_or_else(|| {
                if status_out.status.success() {
                    CloudTaskStatus::Succeeded
                } else {
                    CloudTaskStatus::Running
                }
            });

        match status {
            CloudTaskStatus::Succeeded => {}
            CloudTaskStatus::Failed { message } => {
                return Err(SubstrateError::CloudDispatch(
                    message.unwrap_or_else(|| "codex cloud task failed".into()),
                ));
            }
            _ => {
                return Err(SubstrateError::CloudDispatch(
                    "codex cloud task not ready for harvest".into(),
                ));
            }
        }

        let diff_args = vec!["cloud".into(), "diff".into(), meta.task_id.clone()];
        let diff_out = self.run_cli(&diff_args).await?;
        if !diff_out.status.success() {
            return Err(SubstrateError::CloudDispatch(format!(
                "codex cloud diff failed (exit={}): {}",
                diff_out.status,
                cli_error_text(&diff_out)
            )));
        }

        let diff_text = diff_out.stdout.trim();
        let diff_summary = meta
            .last_summary
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| summarize_diff(diff_text));

        Ok(CloudResult {
            pr_url: meta.task_url.clone(),
            branch: meta.branch.clone(),
            diff_summary,
        })
    }

    async fn run_cli(&self, args: &[String]) -> Result<CodexCommandOutput> {
        self.runner
            .run(&self.config.bin, args)
            .await
            .map_err(|e| SubstrateError::CloudDispatch(e.to_string()))
    }
}

/// Map a Codex Cloud status label to [`CloudTaskStatus`].
pub fn map_codex_status(raw: &str, stderr: String) -> CloudTaskStatus {
    match raw.to_ascii_uppercase().as_str() {
        "PENDING" => CloudTaskStatus::Running,
        "READY" | "APPLIED" => CloudTaskStatus::Succeeded,
        "ERROR" => CloudTaskStatus::Failed {
            message: Some(stderr.trim().to_string()).filter(|s| !s.is_empty()),
        },
        _other => CloudTaskStatus::Running,
    }
}

/// Parse task id from exec stdout (URL or raw id).
pub fn parse_task_id_from_output(stdout_line: &str) -> Option<String> {
    let trimmed = stdout_line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let id = without_query
        .rsplit('/')
        .next()
        .unwrap_or(without_query)
        .trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Strip ANSI escape sequences from CLI output.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.next_if_eq(&'[').is_some() {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// Parse `[STATUS]` from the first line of `codex cloud status` output.
pub fn parse_status_label(stdout: &str) -> Option<String> {
    let clean = strip_ansi(stdout);
    let first = clean.lines().next()?.trim();
    if !first.starts_with('[') {
        return None;
    }
    let end = first.find(']')?;
    let inner = first[1..end].trim();
    let status = inner.split_whitespace().next()?.trim();
    if status.is_empty() {
        None
    } else {
        Some(status.to_string())
    }
}

/// Parse the diff summary line (`+N/-M • K files` or `no diff`).
pub fn parse_summary_line(stdout: &str) -> Option<String> {
    let clean = strip_ansi(stdout);
    clean
        .lines()
        .map(str::trim)
        .find(|line| {
            *line == "no diff"
                || (line.starts_with('+') && line.contains("/-") && line.contains("file"))
        })
        .map(str::to_string)
}

/// Build a short summary from unified diff text.
pub fn summarize_diff(diff: &str) -> String {
    if diff.is_empty() {
        return "no diff".to_string();
    }
    let mut files = 0usize;
    let mut added = 0usize;
    let mut removed = 0usize;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            if !rest.starts_with("/dev/null") {
                files += 1;
            }
            continue;
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            removed += 1;
        }
    }
    if files == 0 && added == 0 && removed == 0 {
        let lines = diff.lines().count();
        return format!("{lines} diff lines");
    }
    format!(
        "+{added}/-{removed} • {files} file{}",
        if files == 1 { "" } else { "s" }
    )
}

fn cli_error_text(output: &CodexCommandOutput) -> String {
    let stderr = output.stderr.trim();
    let stdout = output.stdout.trim();
    if !stderr.is_empty() {
        stderr.to_string()
    } else if !stdout.is_empty() {
        stdout.to_string()
    } else {
        format!("exit {}", output.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_task_id_from_url_and_raw() {
        assert_eq!(
            parse_task_id_from_output("https://chatgpt.com/codex/tasks/task_i_abc123"),
            Some("task_i_abc123".into())
        );
        assert_eq!(
            parse_task_id_from_output("task_i_abc123"),
            Some("task_i_abc123".into())
        );
    }

    #[test]
    fn parse_status_label_handles_brackets() {
        let stdout = "[PENDING] add conformance probe\nenv • 1m ago\nno diff\n";
        assert_eq!(parse_status_label(stdout).as_deref(), Some("PENDING"));
    }

    #[test]
    fn map_codex_statuses() {
        assert_eq!(
            map_codex_status("READY", String::new()),
            CloudTaskStatus::Succeeded
        );
        assert_eq!(
            map_codex_status("ERROR", "boom".into()),
            CloudTaskStatus::Failed {
                message: Some("boom".into())
            }
        );
    }

    #[test]
    fn summarize_diff_counts_lines() {
        let diff = "--- a/foo.rs\n+++ b/foo.rs\n@@\n+line\n";
        let summary = summarize_diff(diff);
        assert!(summary.contains("+1"));
    }
}
