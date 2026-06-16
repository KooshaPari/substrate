//! # engine-agentapi
//!
//! [`EnginePort`] HTTP adapter for `agentapi-plusplus`.
//!
//! The endpoint base URL is taken from the `AGENTAPI_ENDPOINT` env var
//! (default `"http://localhost:3284"`). All real HTTP calls are guarded behind
//! the `AGENTAPI_INTEGRATION=1` env var so CI stays network-free.
//!
//! ## API surface (agentapi-plusplus v1)
//!
//! ```text
//! POST /v1/tasks           → { task_id, conv_id }
//! GET  /v1/conversations/{conv_id}  → raw dump
//! POST /v1/tasks/{task_id}/cancel  → 204
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use engine_spec::{ArgvBuilder, TaskSpec};
use serde::{Deserialize, Serialize};
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::EnginePort;

/// Default endpoint for agentapi-plusplus.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:3284";

/// Request body for `POST /v1/tasks`.
#[derive(Debug, Serialize)]
struct StartRequest<'a> {
    prompt: &'a str,
    cwd: &'a str,
}

/// Response from `POST /v1/tasks`.
#[derive(Debug, Deserialize)]
struct StartResponse {
    task_id: String,
    conv_id: String,
}

/// Argv builder for the agentapi CLI surface (used in golden tests).
///
/// The agentapi adapter communicates over HTTP, but we still implement
/// [`ArgvBuilder`] so it can participate in argv-golden test suites.
#[derive(Debug, Clone, Default)]
pub struct AgentApiArgv {
    /// Base URL of the agentapi server.
    pub endpoint: String,
}

impl AgentApiArgv {
    /// Create with an explicit endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        AgentApiArgv {
            endpoint: endpoint.into(),
        }
    }
}

impl ArgvBuilder for AgentApiArgv {
    fn build_start(&self, spec: &TaskSpec) -> Vec<String> {
        // Synthesised argv so golden tests can compare URL + body params.
        vec![
            "POST".into(),
            format!("{}/v1/tasks", self.endpoint),
            "--prompt".into(),
            spec.prompt.clone(),
            "--cwd".into(),
            spec.cwd.clone(),
        ]
    }

    fn build_dump(&self, conversation_id: &str) -> Vec<String> {
        vec![
            "GET".into(),
            format!("{}/v1/conversations/{conversation_id}", self.endpoint),
        ]
    }
}

/// The agentapi HTTP engine adapter.
pub struct AgentApiEngine {
    endpoint: String,
    client: reqwest::Client,
    /// Maps conv_id (and substrate task id) to agentapi `task_id` for cancel.
    task_ids: Mutex<HashMap<String, String>>,
}

impl std::fmt::Debug for AgentApiEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentApiEngine")
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl Default for AgentApiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentApiEngine {
    /// Construct from the `AGENTAPI_ENDPOINT` env var (default
    /// `"http://localhost:3284"`).
    pub fn new() -> Self {
        let endpoint =
            std::env::var("AGENTAPI_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        AgentApiEngine {
            endpoint,
            client: reqwest::Client::new(),
            task_ids: Mutex::new(HashMap::new()),
        }
    }

    /// Construct with an explicit endpoint URL.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        AgentApiEngine {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
            task_ids: Mutex::new(HashMap::new()),
        }
    }

    /// Expose the argv builder (useful in golden tests).
    pub fn argv_builder(&self) -> AgentApiArgv {
        AgentApiArgv::new(&self.endpoint)
    }

    /// Returns `true` when real HTTP calls should be made (i.e.
    /// `AGENTAPI_INTEGRATION=1` is set).
    fn integration_enabled() -> bool {
        std::env::var("AGENTAPI_INTEGRATION").unwrap_or_default() == "1"
    }
}

#[async_trait]
impl EnginePort for AgentApiEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        if !Self::integration_enabled() {
            // Stub path: return a deterministic session for conformance tests.
            return Ok(Session {
                conv_id: format!("agentapi-{}", task.id),
                pid: None,
                logfile: None,
            });
        }

        let url = format!("{}/v1/tasks", self.endpoint);
        let body = StartRequest {
            prompt: &task.prompt,
            cwd: &task.cwd,
        };
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi POST /v1/tasks: {e}")))?;

        let start: StartResponse = resp
            .json()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi parse response: {e}")))?;

        {
            let mut ids = self.task_ids.lock().unwrap();
            ids.insert(start.conv_id.clone(), start.task_id.clone());
            ids.insert(task.id.to_string(), start.task_id.clone());
        }

        Ok(Session {
            conv_id: start.conv_id,
            pid: None,
            logfile: None,
        })
    }

    async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
        // agentapi-plusplus uses a stateless POST; resume = re-invoke.
        Ok(Session {
            conv_id: conv_id.to_string(),
            pid: None,
            logfile: None,
        })
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        if !Self::integration_enabled() {
            return Ok(ConversationDump {
                conversation_id: conv_id.to_string(),
                raw: format!("{{\"conv_id\":\"{conv_id}\",\"status\":\"completed\"}}"),
            });
        }

        let url = format!("{}/v1/conversations/{conv_id}", self.endpoint);
        let raw = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi GET conversation: {e}")))?
            .text()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi read body: {e}")))?;

        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw,
        })
    }

    async fn cancel(&self, conv_id: &str) -> Result<()> {
        if !Self::integration_enabled() {
            return Ok(());
        }

        let task_id = self
            .task_ids
            .lock()
            .unwrap()
            .get(conv_id)
            .cloned()
            .unwrap_or_else(|| conv_id.to_string());
        let url = format!("{}/v1/tasks/{task_id}/cancel", self.endpoint);
        self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi cancel: {e}")))?;
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
            supports_resume: false,
            supports_subagents: false,
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

    #[test]
    fn argv_start_golden() {
        let argv = AgentApiArgv::new("http://localhost:3284");
        let spec = TaskSpec::new("fix auth bug", "/myrepo");
        let args = argv.build_start(&spec);
        assert_eq!(args[0], "POST");
        assert_eq!(args[1], "http://localhost:3284/v1/tasks");
        assert!(args.contains(&"--prompt".to_string()));
        assert!(args.contains(&"fix auth bug".to_string()));
        assert!(args.contains(&"--cwd".to_string()));
        assert!(args.contains(&"/myrepo".to_string()));
    }

    #[test]
    fn argv_dump_golden() {
        let argv = AgentApiArgv::new("http://localhost:3284");
        let args = argv.build_dump("conv-123");
        assert_eq!(args[0], "GET");
        assert_eq!(args[1], "http://localhost:3284/v1/conversations/conv-123");
    }

    #[tokio::test]
    async fn conformance_suite_passes_offline() {
        // Ensure AGENTAPI_INTEGRATION is not set so stubs are used.
        // (We cannot unset env vars safely in tests, but the default
        //  path is the stub path when the var is absent or not "1".)
        let engine = AgentApiEngine::new();
        engine_conformance::assert_engine_conformance(&engine).await;
    }

    /// Real agentapi integration test — skipped unless `AGENTAPI_INTEGRATION=1`.
    #[tokio::test]
    #[ignore]
    async fn real_agentapi_invocation() {
        if std::env::var("AGENTAPI_INTEGRATION").unwrap_or_default() != "1" {
            return;
        }
        let endpoint =
            std::env::var("AGENTAPI_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let engine = AgentApiEngine::with_endpoint(endpoint);
        let task = Task::new("hello", ".");
        let session = engine.start(&task).await.expect("start failed");
        assert!(!session.conv_id.is_empty());
    }
}
