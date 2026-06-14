//! # engine-forge
//!
//! [`EnginePort`] adapter that drives the `forge` CLI as a subprocess.
//! The binary is taken from the `FORGE_BIN` env var (default `"forge"`),
//! which lets tests point at the bundled fake-forge with zero network.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod parse;

use async_trait::async_trait;
use engine_spec::{ArgvBuilder, TaskSpec};
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::EnginePort;
use tokio::process::Command;

pub use parse::{extract_conversation_id, extract_pr_urls, parse_dump};

/// Argv builder for the forge CLI surface.
#[derive(Debug, Clone, Default)]
pub struct ForgeArgv;

impl ArgvBuilder for ForgeArgv {
    fn build_start(&self, spec: &TaskSpec) -> Vec<String> {
        // forge -p <prompt> --agent forge -C <cwd>
        let agent = spec.agent.clone().unwrap_or_else(|| "forge".to_string());
        vec![
            "-p".into(),
            spec.prompt.clone(),
            "--agent".into(),
            agent,
            "-C".into(),
            spec.cwd.clone(),
        ]
    }

    fn build_dump(&self, conversation_id: &str) -> Vec<String> {
        vec![
            "conversation".into(),
            "dump".into(),
            conversation_id.into(),
        ]
    }
}

/// The forge engine adapter.
#[derive(Debug, Clone)]
pub struct ForgeEngine {
    bin: String,
    argv: ForgeArgv,
}

impl Default for ForgeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeEngine {
    /// Construct from the `FORGE_BIN` env var (default `"forge"`).
    pub fn new() -> Self {
        let bin = std::env::var("FORGE_BIN").unwrap_or_else(|_| "forge".to_string());
        ForgeEngine {
            bin,
            argv: ForgeArgv,
        }
    }

    /// Construct with an explicit binary path (bypasses the env var).
    pub fn with_bin(bin: impl Into<String>) -> Self {
        ForgeEngine {
            bin: bin.into(),
            argv: ForgeArgv,
        }
    }

    async fn run(&self, args: Vec<String>) -> Result<(String, Option<i32>)> {
        let output = Command::new(&self.bin)
            .args(&args)
            .output()
            .await
            .map_err(|e| SubstrateError::Engine(format!("spawn {}: {e}", self.bin)))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        Ok((stdout, output.status.code()))
    }
}

#[async_trait]
impl EnginePort for ForgeEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        let spec = TaskSpec::new(&task.prompt, &task.cwd).with_agent("forge");
        let args = self.argv.build_start(&spec);
        let (stdout, _code) = self.run(args).await?;
        let conv_id = parse::extract_conversation_id(&stdout)
            .unwrap_or_else(parse::fallback_conversation_id);
        Ok(Session {
            conv_id,
            pid: None,
            logfile: None,
        })
    }

    async fn resume(&self, conv_id: &str, prompt: &str) -> Result<Session> {
        // Phase 0: resume re-invokes with the prompt; conv id is preserved.
        let spec = TaskSpec::new(prompt, ".").with_agent("forge");
        let args = self.argv.build_start(&spec);
        let _ = self.run(args).await?;
        Ok(Session {
            conv_id: conv_id.to_string(),
            pid: None,
            logfile: None,
        })
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        let args = self.argv.build_dump(conv_id);
        let (stdout, _code) = self.run(args).await?;
        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: stdout,
        })
    }

    async fn cancel(&self, _conv_id: &str) -> Result<()> {
        // Phase 0: no persistent process to signal.
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        // Phase 0: mailbox wiring is a no-op placeholder.
        Ok(())
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        parse::parse_dump(dump)
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            supports_resume: true,
            supports_subagents: true,
            supports_mcp_import: true,
        }
    }
}
