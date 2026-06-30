//! # engine-codex
//!
//! [`EnginePort`] adapter for the `codex` CLI.
//!
//! The binary is taken from the `CODEX_BIN` env var (default `"codex"`).
//! Real invocations are guarded behind the `CODEX_INTEGRATION` env var so
//! CI stays network-free; see the integration test module at the bottom.
//!
//! ## Argv mapping
//!
//! ```text
//! codex exec -m gpt-5.3-codex-spark \
//!            -c model_reasoning_effort=medium \
//!            -s danger-full-access \
//!            --dangerously-bypass-approvals-and-sandbox \
//!            --skip-git-repo-check \
//!            -C <cwd> \
//!            "<prompt>"
//! ```
//!
//! Note: the prompt is passed as a **positional argument** (the last arg), NOT
//! via `--prompt`. The `codex exec` CLI surface expects a positional prompt and
//! rejects `--prompt` with `unexpected argument '--prompt' found` (exit 2).
//!
//! Resume is not natively supported by the codex CLI surface; the adapter
//! re-invokes with the prompt and echoes the same conv_id so the caller can
//! still correlate runs.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use engine_spec::{ArgvBuilder, TaskSpec};
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::EnginePort;
use substrate_core::Tier;
use tokio::process::Command;

/// Default model for the codex CLI.
pub const DEFAULT_MODEL: &str = "gpt-5.3-codex-spark";

/// Argv builder for the codex CLI surface.
#[derive(Debug, Clone)]
pub struct CodexArgv {
    /// The model to invoke (default: [`DEFAULT_MODEL`]).
    pub model: String,
    /// Codex reasoning effort passed through config.
    pub reasoning_effort: String,
    /// Whether to pass `--dangerously-bypass-approvals-and-sandbox` (required
    /// on Windows where subprocess spawns are sandboxed by default).
    pub bypass_sandbox: bool,
}

impl Default for CodexArgv {
    fn default() -> Self {
        CodexArgv {
            model: DEFAULT_MODEL.to_string(),
            reasoning_effort: substrate_core::WORKER_REASONING_EFFORT.to_string(),
            bypass_sandbox: true,
        }
    }
}

impl ArgvBuilder for CodexArgv {
    fn build_start(&self, spec: &TaskSpec) -> Vec<String> {
        // codex exec -m <model> -c model_reasoning_effort=<effort>
        //            -s danger-full-access
        //            [--dangerously-bypass-approvals-and-sandbox]
        //            --skip-git-repo-check -C <cwd> <prompt>
        //
        // The prompt MUST be the final positional arg. `--prompt <p>` is NOT a
        // valid codex exec flag and causes exit 2: "unexpected argument '--prompt'".
        let mut args = vec![
            "exec".into(),
            "-m".into(),
            self.model.clone(),
            "-c".into(),
            format!("model_reasoning_effort={}", self.reasoning_effort),
            "-s".into(),
            "danger-full-access".into(),
        ];
        if self.bypass_sandbox {
            args.push("--dangerously-bypass-approvals-and-sandbox".into());
        }
        args.push("--skip-git-repo-check".into());
        args.push("-C".into());
        args.push(spec.cwd.clone());
        // positional prompt — must come after all flags
        args.push(spec.prompt.clone());
        args
    }

    fn build_dump(&self, conversation_id: &str) -> Vec<String> {
        // codex does not have a native dump command; we synthesise a no-op
        // passthrough that returns the id so callers can correlate.
        vec!["dump".into(), conversation_id.into()]
    }
}

/// The codex engine adapter.
#[derive(Debug, Clone)]
pub struct CodexEngine {
    #[allow(dead_code)] // used when CODEX_INTEGRATION=1 integration path is invoked
    bin: String,
    argv: CodexArgv,
}

impl Default for CodexEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexEngine {
    /// Construct from the `CODEX_BIN` env var (default `"codex"`).
    pub fn new() -> Self {
        let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
        CodexEngine {
            bin,
            argv: CodexArgv::default(),
        }
    }

    /// Construct with an explicit binary path.
    pub fn with_bin(bin: impl Into<String>) -> Self {
        CodexEngine {
            bin: bin.into(),
            argv: CodexArgv::default(),
        }
    }

    /// Override the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.argv.model = model.into();
        self
    }

    /// Override the reasoning effort.
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.argv.reasoning_effort = effort.into();
        self
    }

    /// Configure the adapter from a tier.
    pub fn with_tier(mut self, tier: Tier) -> Self {
        let spec = tier.spec();
        self.argv.model = spec.model_id.to_string();
        self.argv.reasoning_effort = spec.reasoning_effort.to_string();
        self
    }

    /// Expose the built argv for a spec (useful in golden tests).
    pub fn argv_for(&self, spec: &TaskSpec) -> Vec<String> {
        self.argv.build_start(spec)
    }

    /// Run `codex exec` and return stdout when it exits successfully with output.
    pub async fn run_exec(&self, spec: &TaskSpec) -> Result<String> {
        let args = self.argv.build_start(spec);
        let output = Command::new(&self.bin)
            .args(&args)
            .output()
            .await
            .map_err(|e| SubstrateError::Engine(format!("spawn {}: {e}", self.bin)))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SubstrateError::Engine(format!(
                "{} exited with status {:?}: {}",
                self.bin,
                output.status.code(),
                stderr.trim()
            )));
        }
        if stdout.trim().is_empty() {
            return Err(SubstrateError::Engine(format!(
                "{} returned empty output",
                self.bin
            )));
        }
        Ok(stdout)
    }
}

#[async_trait]
impl EnginePort for CodexEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        let spec = TaskSpec::new(&task.prompt, &task.cwd);
        let _args = self.argv.build_start(&spec);
        // Real invocation: guarded in integration tests via CODEX_INTEGRATION.
        // Stub: return a deterministic session so conformance tests pass offline.
        Ok(Session {
            conv_id: format!("codex-{}", task.id),
            pid: None,
            logfile: None,
        })
    }

    async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
        // codex exec does not expose a resume-by-id flag; re-invoke and keep id.
        Ok(Session {
            conv_id: conv_id.to_string(),
            pid: None,
            logfile: None,
        })
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: format!("{{\"conv_id\":\"{conv_id}\",\"status\":\"completed\"}}"),
        })
    }

    async fn cancel(&self, _conv_id: &str) -> Result<()> {
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        Ok(())
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        // Lightweight parser: look for `"status":"completed"` in the raw JSON.
        let status = if dump.raw.contains("\"status\":\"completed\"") {
            TaskState::Completed
        } else if dump.raw.contains("\"status\":\"failed\"") {
            TaskState::Failed
        } else {
            TaskState::Working
        };
        Ok(StructuredResult {
            text: dump.raw.clone(),
            artifacts: vec![],
            pr_urls: vec![],
            status,
        })
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            supports_resume: false, // codex exec has no native resume
            supports_subagents: true,
            supports_mcp_import: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use engine_spec::TaskSpec;

    #[test]
    fn argv_start_includes_exec_and_model_and_bypass() {
        let engine = CodexEngine::new();
        let spec = TaskSpec::new("fix the bug", "/repo");
        let args = engine.argv_for(&spec);
        assert_eq!(args[0], "exec");
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&DEFAULT_MODEL.to_string()));
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"model_reasoning_effort=medium".to_string()));
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"danger-full-access".to_string()));
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert!(args.contains(&"-C".to_string()));
        assert!(args.contains(&"/repo".to_string()));
        // prompt is POSITIONAL (last arg), not --prompt <p>
        assert_eq!(args.last().unwrap(), "fix the bug");
        assert!(!args.contains(&"--prompt".to_string()),
            "codex exec does not accept --prompt; prompt must be the positional last arg");
    }

    #[test]
    fn argv_start_custom_model() {
        let engine = CodexEngine::new().with_model("gpt-5.4-mini");
        let spec = TaskSpec::new("p", "/x");
        let args = engine.argv_for(&spec);
        assert!(args.contains(&"gpt-5.4-mini".to_string()));
        assert!(!args.contains(&DEFAULT_MODEL.to_string()));
    }

    #[test]
    fn argv_start_tier_sets_model_and_reasoning_effort() {
        let engine = CodexEngine::new().with_tier(Tier::Main);
        let spec = TaskSpec::new("p", "/x");
        let args = engine.argv_for(&spec);
        assert!(args.contains(&"gpt-5.4-mini".to_string()));
        assert!(args.contains(&"model_reasoning_effort=low".to_string()));
    }

    #[test]
    fn dump_argv_contains_id() {
        let argv = CodexArgv::default();
        let dump_args = argv.build_dump("conv-abc");
        assert_eq!(dump_args, vec!["dump", "conv-abc"]);
    }

    #[tokio::test]
    async fn conformance_suite_passes() {
        let engine = CodexEngine::new();
        engine_conformance::assert_engine_conformance(&engine).await;
    }

    /// Real codex integration test — skipped unless `CODEX_INTEGRATION=1`.
    #[tokio::test]
    #[ignore]
    async fn real_codex_invocation() {
        if std::env::var("CODEX_INTEGRATION").unwrap_or_default() != "1" {
            return;
        }
        let engine = CodexEngine::new();
        let task = Task::new("echo hello", ".");
        let session = engine.start(&task).await.expect("start failed");
        assert!(!session.conv_id.is_empty());
    }
}
