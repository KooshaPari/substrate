//! A scripted fake engine for offline testing.
//!
//! `FakeEngine` implements [`EnginePort`] by draining a queue of pre-programmed
//! [`FakeResponse`] values. Tests push scripts in; the supervisor drains them.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;

use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::Result;
use substrate_core::ports::EnginePort;

/// A pre-programmed response for a single engine call.
#[derive(Debug, Clone)]
pub enum FakeResponse {
    /// The call succeeds, returning the given text.
    Ok(String),
    /// The call fails with a resume-400 error (reasoning_details not permitted).
    Resume400,
    /// The call fails with a generic error message.
    Fail(String),
}

/// A scripted engine that returns pre-programmed responses.
///
/// Suitable for offline unit and integration tests — no process or network I/O.
#[derive(Clone, Default)]
pub struct FakeEngine {
    scripts: Arc<Mutex<Vec<FakeResponse>>>,
    /// Number of times `start` or `resume` was called.
    pub call_count: Arc<Mutex<usize>>,
}

impl FakeEngine {
    /// Create a new engine with an empty script queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a scripted response onto the back of the queue.
    pub fn push(&self, resp: FakeResponse) {
        self.scripts.lock().unwrap().push(resp);
    }

    /// Return a session whose `conv_id` encodes the response text (or an error).
    fn next_session(&self, context: &str) -> Result<Session> {
        let resp = {
            let mut q = self.scripts.lock().unwrap();
            if q.is_empty() {
                FakeResponse::Ok(format!("fake-ok:{context}"))
            } else {
                q.remove(0)
            }
        };
        *self.call_count.lock().unwrap() += 1;
        match resp {
            FakeResponse::Ok(text) => Ok(Session {
                conv_id: format!("conv-{}", Uuid::new_v4()),
                pid: None,
                logfile: Some(text),
            }),
            FakeResponse::Resume400 => Err(substrate_core::error::SubstrateError::Engine(
                "resume-400: reasoning_details not permitted".into(),
            )),
            FakeResponse::Fail(msg) => Err(substrate_core::error::SubstrateError::Engine(msg)),
        }
    }
}

#[async_trait]
impl EnginePort for FakeEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        self.next_session(&task.prompt)
    }

    async fn resume(&self, conv_id: &str, prompt: &str) -> Result<Session> {
        let _ = conv_id;
        self.next_session(prompt)
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: r#"{"fake":true}"#.to_string(),
        })
    }

    async fn cancel(&self, _conv_id: &str) -> Result<()> {
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        Ok(())
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        Ok(StructuredResult {
            text: dump.raw.clone(),
            artifacts: vec![],
            pr_urls: vec![],
            status: TaskState::Completed,
        })
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            supports_resume: true,
            supports_subagents: false,
            supports_mcp_import: false,
        }
    }
}
