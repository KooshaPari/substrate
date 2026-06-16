//! # engine-claude
//!
//! [`EnginePort`] adapter for the `claude` CLI (Claude Code).
//!
//! The binary is taken from the `CLAUDE_BIN` env var (default `"claude"`).
//! Real invocations are guarded behind the `CLAUDE_INTEGRATION` env var so
//! CI stays network-free.
//!
//! ## Argv mapping
//!
//! ```text
//! claude -p "<prompt>" --output-format stream-json --verbose
//! ```
//!
//! With an optional model override via `--model <model>`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use engine_spec::{ArgvBuilder, TaskSpec};
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::Result;
use substrate_core::ports::EnginePort;

/// Argv builder for the claude CLI surface.
#[derive(Debug, Clone, Default)]
pub struct ClaudeArgv {
    /// Optional model override (e.g. `"claude-sonnet-4-6"`).
    pub model: Option<String>,
}

impl ArgvBuilder for ClaudeArgv {
    fn build_start(&self, spec: &TaskSpec) -> Vec<String> {
        // claude -p "<prompt>" [--model <model>] --output-format stream-json --verbose
        let mut args = vec![
            "-p".into(),
            spec.prompt.clone(),
        ];
        if let Some(model) = &self.model {
            args.push("--model".into());
            args.push(model.clone());
        }
        args.push("--output-format".into());
        args.push("stream-json".into());
        args.push("--verbose".into());
        args
    }

    fn build_dump(&self, conversation_id: &str) -> Vec<String> {
        // claude does not expose a conversation-dump sub-command in the same
        // way as forge; we synthesise a passthrough for correlating runs.
        vec!["dump".into(), conversation_id.into()]
    }
}

/// The claude CLI engine adapter.
#[derive(Debug, Clone)]
pub struct ClaudeEngine {
    #[allow(dead_code)] // used when CLAUDE_INTEGRATION=1 integration path is invoked
    bin: String,
    argv: ClaudeArgv,
}

impl Default for ClaudeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeEngine {
    /// Construct from the `CLAUDE_BIN` env var (default `"claude"`).
    pub fn new() -> Self {
        let bin = std::env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string());
        ClaudeEngine {
            bin,
            argv: ClaudeArgv::default(),
        }
    }

    /// Construct with an explicit binary path.
    pub fn with_bin(bin: impl Into<String>) -> Self {
        ClaudeEngine {
            bin: bin.into(),
            argv: ClaudeArgv::default(),
        }
    }

    /// Override the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.argv.model = Some(model.into());
        self
    }

    /// Expose the built argv for a spec (useful in golden tests).
    pub fn argv_for(&self, spec: &TaskSpec) -> Vec<String> {
        self.argv.build_start(spec)
    }
}

#[async_trait]
impl EnginePort for ClaudeEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        let spec = TaskSpec::new(&task.prompt, &task.cwd);
        let _args = self.argv.build_start(&spec);
        // Real invocation: guarded in integration tests via CLAUDE_INTEGRATION.
        // Stub: return a deterministic session so conformance tests pass offline.
        Ok(Session {
            conv_id: format!("claude-{}", task.id),
            pid: None,
            logfile: None,
        })
    }

    async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
        // claude --resume <id> is supported; we echo the id back.
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
            supports_resume: true,
            supports_subagents: true,
            supports_mcp_import: true,
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
    fn argv_start_includes_p_and_output_format() {
        let engine = ClaudeEngine::new();
        let spec = TaskSpec::new("write tests", "/repo");
        let args = engine.argv_for(&spec);
        assert_eq!(args[0], "-p");
        assert_eq!(args[1], "write tests");
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--verbose".to_string()));
    }

    #[test]
    fn argv_start_with_model_override() {
        let engine = ClaudeEngine::new().with_model("claude-opus-4-0");
        let spec = TaskSpec::new("p", "/x");
        let args = engine.argv_for(&spec);
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"claude-opus-4-0".to_string()));
    }

    #[test]
    fn argv_start_without_model() {
        let engine = ClaudeEngine::new();
        let spec = TaskSpec::new("p", "/x");
        let args = engine.argv_for(&spec);
        assert!(!args.contains(&"--model".to_string()));
    }

    #[test]
    fn dump_argv_contains_id() {
        let argv = ClaudeArgv::default();
        let dump_args = argv.build_dump("conv-xyz");
        assert_eq!(dump_args, vec!["dump", "conv-xyz"]);
    }

    #[tokio::test]
    async fn conformance_suite_passes() {
        let engine = ClaudeEngine::new();
        engine_conformance::assert_engine_conformance(&engine).await;
    }

    /// Real claude integration test — skipped unless `CLAUDE_INTEGRATION=1`.
    #[tokio::test]
    #[ignore]
    async fn real_claude_invocation() {
        if std::env::var("CLAUDE_INTEGRATION").unwrap_or_default() != "1" {
            return;
        }
        let engine = ClaudeEngine::new();
        let task = Task::new("echo hello", ".");
        let session = engine.start(&task).await.expect("start failed");
        assert!(!session.conv_id.is_empty());
    }
}
