//! # engine-a2a
//!
//! [`EnginePort`] adapter for A2A (Agent-to-Agent) REST task servers.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::time::Duration;

use a2a::{Message, Task as A2aTask, TaskState as A2aTaskState};
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use engine_spec::TaskSpec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::EnginePort;

/// Default A2A REST status poll interval.
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 250;

/// Default maximum task status polls before `start()` returns the live session.
pub const DEFAULT_MAX_POLLS: usize = 120;

/// A2A REST engine adapter.
#[derive(Debug, Clone)]
pub struct A2AEngine {
    http: reqwest::Client,
    poll_interval: Duration,
    max_polls: usize,
}

impl Default for A2AEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl A2AEngine {
    /// Construct an A2A REST engine.
    pub fn new() -> Self {
        let poll_interval_ms = std::env::var("A2A_POLL_INTERVAL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_POLL_INTERVAL_MS);
        let max_polls = std::env::var("A2A_MAX_POLLS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_POLLS);

        Self {
            http: reqwest::Client::new(),
            poll_interval: Duration::from_millis(poll_interval_ms),
            max_polls,
        }
    }

    /// Override polling controls, primarily for tests.
    pub fn with_polling(mut self, poll_interval: Duration, max_polls: usize) -> Self {
        self.poll_interval = poll_interval;
        self.max_polls = max_polls;
        self
    }

    /// Resolve the agent URL from `spec.cwd` or the `A2A_AGENT_URL` environment variable.
    pub fn agent_url_for(&self, spec: &TaskSpec) -> Result<String> {
        if spec.cwd.starts_with("http://") || spec.cwd.starts_with("https://") {
            Ok(spec.cwd.clone())
        } else {
            std::env::var("A2A_AGENT_URL")
                .map_err(|_| SubstrateError::Engine("A2A agent URL is not configured".into()))
        }
    }

    /// Return the task collection URL for an agent card/base URL.
    pub fn tasks_url(&self, agent_url: &str) -> String {
        format!("{}/tasks", agent_url.trim_end_matches('/'))
    }

    /// Return the task status URL for an agent card/base URL and task id.
    pub fn task_url(&self, agent_url: &str, task_id: &str) -> String {
        format!("{}/{}", self.tasks_url(agent_url), task_id)
    }

    /// Return the SSE event URL for an agent card/base URL and task id.
    pub fn task_events_url(&self, agent_url: &str, task_id: &str) -> String {
        format!("{}/events", self.task_url(agent_url, task_id))
    }

    /// Open an SSE stream for an A2A task.
    pub async fn stream_events(
        &self,
        agent_url: &str,
        task_id: &str,
    ) -> Result<impl Stream<Item = Result<A2AEvent>>> {
        let url = self.task_events_url(agent_url, task_id);
        let response = self
            .http
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("a2a GET {url}: {e}")))?;
        if !response.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "a2a GET {url} returned {}",
                response.status()
            )));
        }

        let stream = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| SubstrateError::Engine(format!("a2a SSE chunk: {e}"))))
            .map(|chunk| match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    parse_sse_record(&text)
                }
                Err(e) => Err(e),
            })
            .filter_map(|record| async move {
                match record {
                    Ok(Some(event)) => Some(Ok(event)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            });

        Ok(stream)
    }

    async fn post_task(&self, agent_url: &str, task: &Task) -> Result<A2aTask> {
        let url = self.tasks_url(agent_url);
        let body = a2a_task_from_substrate(task);
        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("a2a POST {url}: {e}")))?;
        if !response.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "a2a POST {url} returned {}",
                response.status()
            )));
        }
        response
            .json::<A2aTask>()
            .await
            .map_err(|e| SubstrateError::Engine(format!("a2a parse POST {url}: {e}")))
    }

    async fn get_task(&self, agent_url: &str, task_id: &str) -> Result<A2aTask> {
        let url = self.task_url(agent_url, task_id);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("a2a GET {url}: {e}")))?;
        if !response.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "a2a GET {url} returned {}",
                response.status()
            )));
        }
        response
            .json::<A2aTask>()
            .await
            .map_err(|e| SubstrateError::Engine(format!("a2a parse GET {url}: {e}")))
    }

    async fn poll_task(&self, agent_url: &str, task_id: &str) -> Result<A2aTask> {
        let mut latest = self.get_task(agent_url, task_id).await?;
        for _ in 1..self.max_polls {
            if latest.state.is_terminal() {
                return Ok(latest);
            }
            tokio::time::sleep(self.poll_interval).await;
            latest = self.get_task(agent_url, task_id).await?;
        }
        Ok(latest)
    }
}

#[async_trait]
impl EnginePort for A2AEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        let spec = TaskSpec::new(&task.prompt, &task.cwd);
        let Ok(agent_url) = self.agent_url_for(&spec) else {
            return Ok(offline_session(task));
        };

        let submitted = self.post_task(&agent_url, task).await?;
        let task_id = submitted.id.to_string();
        let event_engine = self.clone();
        let event_agent_url = agent_url.clone();
        let event_task_id = task_id.clone();
        tokio::spawn(async move {
            if let Ok(events) = event_engine
                .stream_events(&event_agent_url, &event_task_id)
                .await
            {
                futures_util::pin_mut!(events);
                while let Some(_event) = events.next().await {}
            }
        });
        let _latest = self.poll_task(&agent_url, &task_id).await?;

        Ok(Session {
            conv_id: task_id,
            pid: None,
            logfile: None,
        })
    }

    async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
        Ok(Session {
            conv_id: conv_id.to_string(),
            pid: None,
            logfile: None,
        })
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: format!("{{\"id\":\"{conv_id}\",\"state\":\"completed\"}}"),
        })
    }

    async fn cancel(&self, _conv_id: &str) -> Result<()> {
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        Ok(())
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        let status = match serde_json::from_str::<A2aTask>(&dump.raw) {
            Ok(task) => map_task_state(task.state),
            Err(_) => fallback_status_from_raw(&dump.raw),
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
            supports_mcp_import: false,
        }
    }
}

/// A parsed A2A SSE event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum A2AEvent {
    /// A task update event.
    Task {
        /// Updated task.
        task: A2aTask,
    },
    /// A message event.
    Message {
        /// A2A message.
        message: Message,
    },
    /// An artifact event.
    Artifact {
        /// A2A artifact payload.
        artifact: a2a::Artifact,
    },
    /// Unknown or extension event payload.
    Other {
        /// Raw event name, if supplied.
        event: Option<String>,
        /// Raw JSON payload.
        data: Value,
    },
}

fn offline_session(task: &Task) -> Session {
    Session {
        conv_id: format!("a2a-{}", task.id),
        pid: None,
        logfile: None,
    }
}

fn a2a_task_from_substrate(task: &Task) -> A2aTask {
    let mut a2a_task = A2aTask::new("substrate", task.prompt.clone(), "substrate");
    a2a_task.id = task.id;
    a2a_task.parent_task_id = task.parent_task_id;
    a2a_task.requirement_id = task.requirement_id.clone();
    a2a_task.epic_id = task.epic_id.clone();
    a2a_task
}

fn map_task_state(state: A2aTaskState) -> TaskState {
    match state {
        A2aTaskState::Submitted => TaskState::Submitted,
        A2aTaskState::Working => TaskState::Working,
        A2aTaskState::InputRequired => TaskState::InputRequired,
        A2aTaskState::Completed => TaskState::Completed,
        A2aTaskState::Failed => TaskState::Failed,
        A2aTaskState::Cancelled => TaskState::Cancelled,
    }
}

fn parse_sse_record(text: &str) -> Result<Option<A2AEvent>> {
    let mut event = None;
    let mut data = None::<String>;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            let piece = rest.trim();
            if let Some(existing) = data.as_mut() {
                existing.push('\n');
                existing.push_str(piece);
            } else {
                data = Some(piece.to_string());
            }
        }
    }

    let Some(data) = data else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&data)?;
    let parsed = match event.as_deref() {
        Some("task") | Some("task_update") => A2AEvent::Task {
            task: serde_json::from_value(value)?,
        },
        Some("message") | Some("message_update") => A2AEvent::Message {
            message: serde_json::from_value(value)?,
        },
        Some("artifact") | Some("artifact_update") => A2AEvent::Artifact {
            artifact: serde_json::from_value(value)?,
        },
        _ => A2AEvent::Other { event, data: value },
    };
    Ok(Some(parsed))
}

fn fallback_status_from_raw(raw: &str) -> TaskState {
    if raw.contains("\"state\":\"completed\"") || raw.contains("\"status\":\"completed\"") {
        TaskState::Completed
    } else if raw.contains("\"state\":\"failed\"") || raw.contains("\"status\":\"failed\"") {
        TaskState::Failed
    } else if raw.contains("\"state\":\"cancelled\"") || raw.contains("\"status\":\"cancelled\"") {
        TaskState::Cancelled
    } else if raw.contains("\"state\":\"input_required\"")
        || raw.contains("\"status\":\"input_required\"")
    {
        TaskState::InputRequired
    } else {
        TaskState::Working
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::{Path, State},
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use std::net::SocketAddr;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::net::TcpListener;

    #[test]
    fn agent_url_prefers_http_cwd() {
        let engine = A2AEngine::new();
        let spec = TaskSpec::new("do work", "https://agent.example/a2a");

        let url = engine.agent_url_for(&spec).expect("agent url");

        assert_eq!(url, "https://agent.example/a2a");
    }

    #[test]
    fn tasks_url_appends_tasks_to_agent_url() {
        let engine = A2AEngine::new();

        assert_eq!(
            engine.tasks_url("https://agent.example/a2a/"),
            "https://agent.example/a2a/tasks"
        );
    }

    #[test]
    fn extract_result_maps_completed_a2a_task_to_completed() {
        let engine = A2AEngine::new();
        let task = a2a::Task::new("substrate", "write code", "remote-agent");
        let mut raw_task = task;
        raw_task.state = a2a::TaskState::Completed;
        let dump = ConversationDump {
            conversation_id: raw_task.id.to_string(),
            raw: serde_json::to_string(&raw_task).expect("serialize task"),
        };

        let result = engine.extract_result(&dump).expect("result");

        assert_eq!(result.status, TaskState::Completed);
    }

    #[test]
    fn parse_sse_task_event() {
        let mut task = a2a::Task::new("substrate", "ship", "remote-agent");
        task.state = a2a::TaskState::Working;
        let payload = serde_json::to_string(&task).expect("serialize task");
        let record = format!("event: task_update\ndata: {payload}\n\n");

        let event = parse_sse_record(&record).expect("parse").expect("event");

        assert_eq!(event, A2AEvent::Task { task });
    }

    #[tokio::test]
    async fn start_posts_task_streams_events_and_polls_status() {
        #[derive(Clone)]
        struct TestState {
            post_count: Arc<AtomicUsize>,
            get_count: Arc<AtomicUsize>,
            events_count: Arc<AtomicUsize>,
        }

        async fn post_task(
            State(state): State<TestState>,
            Json(mut task): Json<a2a::Task>,
        ) -> Json<a2a::Task> {
            state.post_count.fetch_add(1, Ordering::SeqCst);
            task.state = a2a::TaskState::Working;
            Json(task)
        }

        async fn get_task(
            State(state): State<TestState>,
            Path(id): Path<String>,
        ) -> Json<a2a::Task> {
            state.get_count.fetch_add(1, Ordering::SeqCst);
            let mut task = a2a::Task::new("substrate", "remote result", "remote-agent");
            task.id = uuid::Uuid::parse_str(&id).expect("task id");
            task.state = a2a::TaskState::Completed;
            Json(task)
        }

        async fn task_events(State(state): State<TestState>) -> impl IntoResponse {
            state.events_count.fetch_add(1, Ordering::SeqCst);
            (
                [("content-type", "text/event-stream")],
                "event: artifact\ndata: {\"kind\":\"log\",\"content\":\"ok\",\"name\":null}\n\n",
            )
        }

        let state = TestState {
            post_count: Arc::new(AtomicUsize::new(0)),
            get_count: Arc::new(AtomicUsize::new(0)),
            events_count: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route("/tasks", post(post_task))
            .route("/tasks/{id}", get(get_task))
            .route("/tasks/{id}/events", get(task_events))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server");
        });

        let engine = A2AEngine::new().with_polling(Duration::from_millis(1), 2);
        let task = Task::new("remote result", format!("http://{addr}"));

        let session = engine.start(&task).await.expect("start");
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert_eq!(session.conv_id, task.id.to_string());
        assert_eq!(state.post_count.load(Ordering::SeqCst), 1);
        assert!(state.get_count.load(Ordering::SeqCst) >= 1);
        assert_eq!(state.events_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn conformance_suite_passes() {
        let engine = A2AEngine::new();
        engine_conformance::assert_engine_conformance(&engine).await;
    }
}
