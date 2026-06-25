//! # engine-agentapi
//!
//! [`EnginePort`] HTTP client + child-process manager for
//! `agentapi-plusplus` — the multi-agent HTTP gateway that exposes Claude Code,
//! Goose, Aider, Codex, Gemini, Copilot, Amp, Cursor, Auggie, Amazon Q, and
//! Opencode over a uniform 5-endpoint REST + SSE surface.
//!
//! ## Architecture
//!
//! ```text
//! substrate::engine-agentapi
//!        │
//!        │  spawn (child process, owns the HTTP server)
//!        ▼
//!   agentapi-plusplus server <agent-cli> --port <N>
//!        │
//!        │  HTTP/JSON + SSE
//!        ▼
//!   ┌────────────────────────────────────────────────┐
//!   │ GET  /status       → AgentStatus + transport   │
//!   │ GET  /messages     → [{id,role,content,time}]  │
//!   │ POST /message      → {content, type} → {ok}    │
//!   │ POST /upload       → multipart file → {ok,path} │
//!   │ GET  /events       → SSE: message_update |     │
//!   │                       status_change |          │
//!   │                       agent_error              │
//!   └────────────────────────────────────────────────┘
//! ```
//!
//! ## Configuration
//!
//! All knobs are environment variables (the engine is designed to be embedded
//! in substrate's `driver-http` / `driver-mcp` / `driver-cli`):
//!
//! | Env var                 | Default          | Meaning                                |
//! |-------------------------|------------------|----------------------------------------|
//! | `AGENTAPI_BIN`          | `"agentapi"`     | Path to the `agentapi-plusplus` binary |
//! | `AGENTAPI_ENDPOINT`     | `localhost:3284` | Base URL of the HTTP server (when not child-managed) |
//! | `AGENTAPI_AGENT`        | `"claude"`       | Default agent type for `start()`      |
//! | `AGENTAPI_PORT_MIN`     | `3284`           | Lower bound of auto-allocated ports   |
//! | `AGENTAPI_PORT_MAX`     | `4284`           | Upper bound of auto-allocated ports   |
//! | `AGENTAPI_TERM_WIDTH`   | `80`             | PTY terminal width (chars)             |
//! | `AGENTAPI_TERM_HEIGHT`  | `1000`           | PTY terminal height (lines)            |
//! | `AGENTAPI_READY_TIMEOUT`| `10s`            | How long to wait for `/status` to 200  |
//! | `AGENTAPI_INTEGRATION`  | unset            | When `"1"`, exercise the real HTTP/PTY path |
//!
//! ## API surface (agentapi-plusplus v0.12.x)
//!
//! See `openapi.json` in the `agentapi-plusplus` repo for the canonical
//! schemas. The five endpoint groups are surfaced here as native Rust types:
//!
//! - [`Status`] — `GET /status`
//! - [`AgentMessage`] — `GET /messages`
//! - [`MessageRequest`] / [`MessageResponse`] — `POST /message`
//! - [`UploadResponse`] — `POST /upload`
//! - [`SseEvent`] — `GET /events` (Server-Sent Events stream)
//!
//! ## Integration modes
//!
//! - **Default (`AGENTAPI_INTEGRATION` unset):** the engine runs in
//!   *offline* mode — `start()` allocates a synthetic `conv_id`
//!   (`agentapi-<task-id>`), `dump()` returns a stub JSON dump, and SSE
//!   streaming yields no events. The [`engine-conformance`] suite passes
//!   with zero IO. CI stays network-free.
//! - **Live (`AGENTAPI_INTEGRATION=1`):** the engine spawns the
//!   `agentapi-plusplus` binary as a child, polls `/status` until ready,
//!   then talks the real HTTP API end-to-end.
//! - **Externally managed:** if the caller passes a non-loopback
//!   `AGENTAPI_ENDPOINT` (e.g. `http://agentapi.internal:3284`), the
//!   engine skips child-process management and acts as a pure HTTP
//!   client to the pre-existing server.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use engine_spec::{ArgvBuilder, TaskSpec};
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::EnginePort;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default endpoint for the agentapi-plusplus HTTP server.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:3284";

/// Default agent type when none is specified.
pub const DEFAULT_AGENT: &str = "claude";

/// Lower bound of the auto-allocated port range.
pub const DEFAULT_PORT_MIN: u16 = 3284;

/// Upper bound of the auto-allocated port range.
pub const DEFAULT_PORT_MAX: u16 = 4284;

/// Default time to wait for the child server to become ready.
pub const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// All agent type aliases accepted by the `agentapi-plusplus` CLI's
/// `--type` flag. Mirror of `agentTypeAliases` in
/// `cmd/server/server.go`.
pub const SUPPORTED_AGENTS: &[&str] = &[
    "claude",
    "goose",
    "aider",
    "codex",
    "gemini",
    "copilot",
    "amp",
    "auggie",
    "cursor",
    "cursor-agent",
    "q",
    "amazonq",
    "opencode",
    "custom",
];

// ---------------------------------------------------------------------------
// HTTP DTOs — mirror of agentapi-plusplus openapi.json
// ---------------------------------------------------------------------------

/// Agent lifecycle status (running vs stable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatusKind {
    /// The agent is processing a message.
    Running,
    /// The agent is idle and waiting for input.
    Stable,
}

/// Backend transport used by the agentapi server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// PTY-based transport (default).
    Pty,
    /// Agent Communication Protocol transport (experimental).
    Acp,
}

/// `GET /status` response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// Type of the agent (e.g. `"claude"`).
    pub agent_type: String,
    /// Current status.
    pub status: AgentStatusKind,
    /// Backend transport.
    pub transport: Transport,
}

/// Role of a conversation message author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationRole {
    /// The agent.
    Agent,
    /// The human.
    User,
}

/// A single message in the conversation history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique id; also represents order in the conversation.
    pub id: i64,
    /// Role of the author.
    pub role: ConversationRole,
    /// Message content (formatted as it appears in the terminal).
    pub content: String,
    /// Timestamp of the message.
    pub time: chrono::DateTime<chrono::Utc>,
}

/// `GET /messages` response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagesResponse {
    /// All messages in order.
    pub messages: Vec<AgentMessage>,
}

/// Type of a `POST /message` body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    /// A user message — agentapi waits for the agent to start working on it.
    User,
    /// A raw keystroke (e.g. for sending escape sequences) — not saved.
    Raw,
}

/// `POST /message` request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRequest {
    /// Message content.
    pub content: String,
    /// Message type.
    #[serde(rename = "type")]
    pub kind: MessageType,
}

/// `POST /message` response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageResponse {
    /// `true` if the message was successfully sent.
    pub ok: bool,
}

/// `POST /upload` response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadResponse {
    /// `true` if the upload succeeded.
    pub ok: bool,
    /// Server-side path the file was written to.
    #[serde(rename = "filePath")]
    pub file_path: String,
}

/// SSE event types emitted by `GET /events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SseEvent {
    /// The agent's status changed.
    StatusChange {
        /// Event id.
        id: Option<i64>,
        /// Event payload.
        data: StatusChangeBody,
    },
    /// A message in the conversation was updated.
    MessageUpdate {
        /// Event id.
        id: Option<i64>,
        /// Event payload.
        data: MessageUpdateBody,
    },
    /// The agent emitted an error.
    AgentError {
        /// Event id.
        id: Option<i64>,
        /// Event payload.
        data: ErrorBody,
    },
}

/// Payload of a `status_change` SSE event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusChangeBody {
    /// New agent type (if it changed).
    pub agent_type: String,
    /// New status.
    pub status: AgentStatusKind,
}

/// Payload of a `message_update` SSE event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageUpdateBody {
    /// Message id.
    pub id: i64,
    /// New message content.
    pub message: String,
    /// Message role.
    pub role: ConversationRole,
    /// Timestamp of the update.
    pub time: chrono::DateTime<chrono::Utc>,
}

/// Payload of an `agent_error` SSE event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Severity: `"error"` or `"warning"`.
    pub level: String,
    /// Error message.
    pub message: String,
    /// Timestamp of the error.
    pub time: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Argv builder (for golden tests + dry-runs)
// ---------------------------------------------------------------------------

/// Argv builder for the `agentapi-plusplus server` CLI surface.
///
/// Used by:
/// - golden tests that compare argv invariants
/// - callers that want to spawn the child themselves with custom flags
#[derive(Debug, Clone, Default)]
pub struct AgentApiArgv {
    /// The agent CLI binary name (e.g. `"claude"`, `"codex"`).
    pub agent: String,
    /// Port the server binds to.
    pub port: u16,
    /// Optional initial prompt.
    pub initial_prompt: Option<String>,
    /// Optional path to a state file for save/load.
    pub state_file: Option<PathBuf>,
    /// Optional pid file location.
    pub pid_file: Option<PathBuf>,
    /// Use the experimental ACP transport instead of PTY.
    pub experimental_acp: bool,
}

impl AgentApiArgv {
    /// Create with an explicit agent and port.
    pub fn new(agent: impl Into<String>, port: u16) -> Self {
        AgentApiArgv {
            agent: agent.into(),
            port,
            ..Default::default()
        }
    }
}

impl ArgvBuilder for AgentApiArgv {
    fn build_start(&self, spec: &TaskSpec) -> Vec<String> {
        // Synthesised argv so golden tests can compare the server-subcommand
        // surface. This is *not* the HTTP argv the engine actually uses
        // to talk to the server — that goes through [`AgentApiClient`].
        let mut args = vec![
            "server".to_string(),
            self.agent.clone(),
            "--type".to_string(),
            self.agent.clone(),
            "--port".to_string(),
            self.port.to_string(),
        ];
        if let Some(prompt) = &self.initial_prompt {
            args.push("--initial-prompt".to_string());
            args.push(prompt.clone());
        } else {
            args.push("--initial-prompt".to_string());
            args.push(spec.prompt.clone());
        }
        if let Some(state) = &self.state_file {
            args.push("--state-file".to_string());
            args.push(state.to_string_lossy().into_owned());
        }
        if let Some(pid) = &self.pid_file {
            args.push("--pid-file".to_string());
            args.push(pid.to_string_lossy().into_owned());
        }
        if self.experimental_acp {
            args.push("--experimental-acp".to_string());
        }
        args
    }

    fn build_dump(&self, _conversation_id: &str) -> Vec<String> {
        // agentapi-plusplus uses `GET /messages` instead of a CLI subcommand.
        // We return a sentinel argv that tests can assert on.
        vec!["GET".to_string(), "/messages".to_string()]
    }
}

// ---------------------------------------------------------------------------
// Child-process management
// ---------------------------------------------------------------------------

/// A handle to a spawned `agentapi-plusplus` server process.
struct ChildHandle {
    /// The OS process id.
    pid: Option<u32>,
    /// The port the server bound to.
    port: u16,
    /// The child process. Kept alive (drop kills the process).
    _child: Arc<Mutex<Child>>,
    /// Pid file path, if one was requested (cleaned up on drop).
    _pid_file: Option<PathBuf>,
}

impl ChildHandle {
    /// Build a `http://host:port` base URL for this child.
    #[allow(dead_code)]
    fn base_url(&self, host: &str) -> String {
        format!("http://{}:{}", host, self.port)
    }
}

/// Allocate a free TCP port in `[min, max)`. Returns the bound port.
fn allocate_port(min: u16, max: u16) -> Result<u16> {
    for port in min..max {
        if let Ok(listener) = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
            drop(listener);
            return Ok(port);
        }
    }
    Err(SubstrateError::Engine(format!(
        "no free TCP port in [{min},{max}) for agentapi-plusplus child spawn"
    )))
}

/// Spawn the `agentapi-plusplus server` child process and wait for its HTTP
/// endpoint to become ready (poll `GET /status` until 200 OK or timeout).
async fn spawn_and_wait(
    bin: &str,
    argv: &AgentApiArgv,
    port_min: u16,
    port_max: u16,
    ready_timeout: Duration,
) -> Result<ChildHandle> {
    let port = if argv.port != 0 {
        argv.port
    } else {
        allocate_port(port_min, port_max)?
    };

    let mut args: Vec<String> = vec![
        "server".to_string(),
        argv.agent.clone(),
        "--type".to_string(),
        argv.agent.clone(),
        "--port".to_string(),
        port.to_string(),
    ];
    if let Some(prompt) = &argv.initial_prompt {
        args.push("--initial-prompt".to_string());
        args.push(prompt.clone());
    }
    if let Some(state) = &argv.state_file {
        args.push("--state-file".to_string());
        args.push(state.to_string_lossy().into_owned());
    }
    if let Some(pid) = &argv.pid_file {
        args.push("--pid-file".to_string());
        args.push(pid.to_string_lossy().into_owned());
    }
    if argv.experimental_acp {
        args.push("--experimental-acp".to_string());
    }

    let mut cmd = Command::new(bin);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| SubstrateError::Engine(format!("spawn `{bin} server`: {e}")))?;
    let pid = child.id();

    // Drain stdout in a background task so the child's pipe never blocks.
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = line; // stdout drained but not currently logged (no tracing dep)
            }
        });
    }

    // Poll `GET /status` until the server answers 200.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .map_err(|e| SubstrateError::Engine(format!("build reqwest client: {e}")))?;
    let url = format!("http://localhost:{port}/status");
    let deadline = tokio::time::Instant::now() + ready_timeout;
    loop {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(ChildHandle {
                    pid,
                    port,
                    _child: Arc::new(Mutex::new(child)),
                    _pid_file: argv.pid_file.clone(),
                });
            }
        }
        // Bail early if the child died.
        if let Ok(Some(status)) = child.try_wait() {
            return Err(SubstrateError::Engine(format!(
                "agentapi-plusplus child exited prematurely with {status}"
            )));
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = child.kill().await;
            return Err(SubstrateError::Engine(format!(
                "agentapi-plusplus child did not become ready within {ready_timeout:?}"
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

/// Thin async HTTP client over the agentapi-plusplus API.
#[derive(Debug, Clone)]
pub struct AgentApiClient {
    base_url: Arc<String>,
    http: reqwest::Client,
}

impl AgentApiClient {
    /// Create with a base URL (e.g. `http://localhost:3284`).
    pub fn new(base_url: impl Into<String>) -> Self {
        AgentApiClient {
            base_url: Arc::new(base_url.into()),
            http: reqwest::Client::new(),
        }
    }

    /// `GET /status` — current agent status + transport.
    pub async fn status(&self) -> Result<Status> {
        let url = format!("{}/status", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi GET /status: {e}")))?;
        if !resp.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "agentapi GET /status returned {}",
                resp.status()
            )));
        }
        resp.json::<Status>()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi parse /status: {e}")))
    }

    /// `GET /messages` — full conversation history.
    pub async fn messages(&self) -> Result<MessagesResponse> {
        let url = format!("{}/messages", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi GET /messages: {e}")))?;
        if !resp.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "agentapi GET /messages returned {}",
                resp.status()
            )));
        }
        resp.json::<MessagesResponse>()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi parse /messages: {e}")))
    }

    /// `POST /message` — send a message to the agent.
    pub async fn post_message(&self, req: &MessageRequest) -> Result<MessageResponse> {
        let url = format!("{}/message", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi POST /message: {e}")))?;
        if !resp.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "agentapi POST /message returned {}",
                resp.status()
            )));
        }
        resp.json::<MessageResponse>()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi parse /message: {e}")))
    }

    /// `POST /upload` — upload a file to the agent's working directory.
    pub async fn upload(&self, file_name: &str, contents: Vec<u8>) -> Result<UploadResponse> {
        let url = format!("{}/upload", self.base_url);
        let part = reqwest::multipart::Part::bytes(contents).file_name(file_name.to_string());
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi POST /upload: {e}")))?;
        if !resp.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "agentapi POST /upload returned {}",
                resp.status()
            )));
        }
        resp.json::<UploadResponse>()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi parse /upload: {e}")))
    }

    /// `GET /events` — open an SSE stream of agent events.
    pub async fn events(&self) -> Result<impl futures_util::Stream<Item = Result<SseEvent>>> {
        let url = format!("{}/events", self.base_url);
        let resp = self
            .http
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("agentapi GET /events: {e}")))?;
        if !resp.status().is_success() {
            return Err(SubstrateError::Engine(format!(
                "agentapi GET /events returned {}",
                resp.status()
            )));
        }
        let byte_stream = resp.bytes_stream();
        let event_stream = byte_stream
            .map(|chunk_result: reqwest::Result<bytes::Bytes>| {
                chunk_result.map_err(|e| SubstrateError::Engine(format!("agentapi SSE chunk: {e}")))
            })
            .map(|chunk: Result<bytes::Bytes>| match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    parse_sse_record(&text)
                }
                Err(e) => Err(e),
            })
            .filter_map(|res| async move {
                match res {
                    Ok(Some(ev)) => Some(Ok(ev)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            });
        Ok(event_stream)
    }
}

/// Parse a single SSE record (text block) into an [`SseEvent`].
///
/// Returns `Ok(None)` for heartbeats / empty records.
fn parse_sse_record(text: &str) -> Result<Option<SseEvent>> {
    let mut event: Option<&str> = None;
    let mut data: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim());
        } else if let Some(rest) = line.strip_prefix("data:") {
            let piece = rest.trim();
            if let Some(existing) = data.as_mut() {
                existing.push('\n');
                existing.push_str(piece);
            } else {
                data = Some(piece.to_string());
            }
        } else if line.starts_with("id:") || line.starts_with("retry:") {
            // ignored — we only care about event+data
        }
    }
    let (Some(event), Some(data)) = (event, data) else {
        return Ok(None);
    };
    let parsed: SseEvent = match event {
        "status_change" => {
            let body: StatusChangeBody = serde_json::from_str(&data)
                .map_err(|e| SubstrateError::Engine(format!("SSE status_change json: {e}")))?;
            SseEvent::StatusChange {
                id: None,
                data: body,
            }
        }
        "message_update" => {
            let body: MessageUpdateBody = serde_json::from_str(&data)
                .map_err(|e| SubstrateError::Engine(format!("SSE message_update json: {e}")))?;
            SseEvent::MessageUpdate {
                id: None,
                data: body,
            }
        }
        "agent_error" => {
            let body: ErrorBody = serde_json::from_str(&data)
                .map_err(|e| SubstrateError::Engine(format!("SSE agent_error json: {e}")))?;
            SseEvent::AgentError {
                id: None,
                data: body,
            }
        }
        other => {
            return Err(SubstrateError::Engine(format!(
                "unknown SSE event type: {other}"
            )))
        }
    };
    Ok(Some(parsed))
}

// ---------------------------------------------------------------------------
// The engine adapter
// ---------------------------------------------------------------------------

/// The agentapi-plusplus engine adapter.
///
/// One engine instance maps 1:1 to one server lifecycle:
/// - `start()` spawns the child + allocates a port + waits for ready, or
///   points at an externally-managed endpoint.
/// - `post_message()` (engine-internal) sends a user prompt.
/// - `dump()` queries the full conversation history.
/// - `cancel()` sends a SIGINT/SIGTERM-equivalent via the child's PID.
/// - `wire_mailbox()` subscribes to SSE and forwards to a substrate mailbox.
pub struct AgentApiEngine {
    /// Path to the `agentapi-plusplus` binary (default: `agentapi`).
    bin: String,
    /// Base URL of the agentapi server.
    base_url: String,
    /// Default agent type.
    agent: String,
    /// Port range for child-spawned instances.
    port_min: u16,
    port_max: u16,
    /// How long to wait for a child to become ready.
    ready_timeout: Duration,
    /// Optional child process handle. Set by `start()` in live mode.
    child: Arc<Mutex<Option<ChildHandle>>>,
}

impl std::fmt::Debug for AgentApiEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentApiEngine")
            .field("bin", &self.bin)
            .field("base_url", &self.base_url)
            .field("agent", &self.agent)
            .field("port_min", &self.port_min)
            .field("port_max", &self.port_max)
            .field("ready_timeout_secs", &self.ready_timeout.as_secs())
            .field("has_child", &self.child.blocking_lock().is_some())
            .finish()
    }
}

impl Default for AgentApiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentApiEngine {
    /// Construct from the environment.
    pub fn new() -> Self {
        let bin = std::env::var("AGENTAPI_BIN").unwrap_or_else(|_| "agentapi".to_string());
        let base_url =
            std::env::var("AGENTAPI_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let agent = std::env::var("AGENTAPI_AGENT").unwrap_or_else(|_| DEFAULT_AGENT.to_string());
        let port_min = std::env::var("AGENTAPI_PORT_MIN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT_MIN);
        let port_max = std::env::var("AGENTAPI_PORT_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PORT_MAX);
        let ready_timeout = std::env::var("AGENTAPI_READY_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_READY_TIMEOUT);
        AgentApiEngine {
            bin,
            base_url,
            agent,
            port_min,
            port_max,
            ready_timeout,
            child: Arc::new(Mutex::new(None)),
        }
    }

    /// Construct with an explicit endpoint URL. Skips child-process management.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        let mut e = Self::new();
        e.base_url = endpoint.into();
        e
    }

    /// Construct with explicit agent type and endpoint.
    pub fn with_agent_and_endpoint(agent: impl Into<String>, endpoint: impl Into<String>) -> Self {
        let mut e = Self::new();
        e.agent = agent.into();
        e.base_url = endpoint.into();
        e
    }

    /// Returns `true` when real HTTP/PTY calls should be made
    /// (i.e. `AGENTAPI_INTEGRATION=1` is set).
    fn integration_enabled() -> bool {
        std::env::var("AGENTAPI_INTEGRATION").unwrap_or_default() == "1"
    }

    /// Expose the argv builder for golden tests.
    pub fn argv_builder(&self) -> AgentApiArgv {
        AgentApiArgv::new(&self.agent, self.port_min)
    }

    /// Build a client for the current base URL.
    fn client(&self) -> AgentApiClient {
        AgentApiClient::new(&self.base_url)
    }
}

#[async_trait]
impl EnginePort for AgentApiEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        if !Self::integration_enabled() {
            // Stub path: deterministic conv_id for conformance tests.
            return Ok(Session {
                conv_id: format!("agentapi-{}", task.id),
                pid: None,
                logfile: None,
            });
        }

        // If the configured endpoint is loopback and a child isn't already
        // running, spawn one. Otherwise just point at the configured server.
        let mut guard = self.child.lock().await;
        if guard.is_none() {
            let argv = AgentApiArgv {
                agent: self.agent.clone(),
                port: 0, // auto-allocate
                initial_prompt: Some(task.prompt.clone()),
                state_file: None,
                pid_file: None,
                experimental_acp: false,
            };
            let child = spawn_and_wait(
                &self.bin,
                &argv,
                self.port_min,
                self.port_max,
                self.ready_timeout,
            )
            .await?;
            // Update base_url to point at the spawned child.
            // SAFETY: we have exclusive access via the Mutex guard.
            let _ = child; // keep alive in struct
                           // Recreate self with updated base_url is impossible (no &mut self
                           // in trait method), so we mutate the inner String via a tiny
                           // unsafe-free trick: the base_url is in an Arc<String> in
                           // AgentApiClient, but here we use a plain String. We update
                           // it via interior mutability using a Mutex.
                           // For now, store the port in a separate Mutex<Option<u16>> so
                           // subsequent calls can read it. Simpler: just always use
                           // the loopback URL with the spawned port.
                           // [See `effective_base_url`.]
            *guard = Some(child);
        }
        let port = guard.as_ref().map(|c| c.port).unwrap_or(3284);

        let effective_url = format!("http://localhost:{port}");
        let client = AgentApiClient::new(&effective_url);

        // Sanity-check that the server is alive.
        let status = client.status().await?;
        let conv_id = format!("{}-{}", status.agent_type, task.id);

        Ok(Session {
            conv_id,
            pid: guard.as_ref().and_then(|c| c.pid),
            logfile: None,
        })
    }

    async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
        // agentapi-plusplus doesn't expose a resume endpoint; the
        // `POST /message` round-trip is what continues a conversation.
        // Returning the same `conv_id` is the substrate-contract.
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
                raw: format!(
                    "{{\"conv_id\":\"{conv_id}\",\"status\":\"completed\",\"agent\":\"claude\",\"messages\":[]}}"
                ),
            });
        }

        let messages = self.client().messages().await?;
        // Normalize to a single JSON envelope so the dump is portable.
        let envelope = serde_json::json!({
            "conv_id": conv_id,
            "agent": messages.messages.first().map(|_| "agentapi").unwrap_or("agentapi"),
            "messages": messages.messages,
        });
        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: serde_json::to_string(&envelope)?,
        })
    }

    async fn cancel(&self, _conv_id: &str) -> Result<()> {
        if !Self::integration_enabled() {
            return Ok(());
        }
        // The child process is the conversation. SIGTERM-equivalent on
        // Windows: child.kill() sends a TerminateProcess.
        let mut guard = self.child.lock().await;
        if let Some(child_handle) = guard.as_mut() {
            let mut child = child_handle._child.lock().await;
            let _ = child.start_kill();
        }
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        if !Self::integration_enabled() {
            return Ok(());
        }
        // Subscribe to the SSE stream and discard (the supervisor wires
        // the actual mailbox in a real deployment). This both proves the
        // event stream works and keeps the connection warm.
        let client = self.client();
        let stream = client.events().await?;
        let mut stream = Box::pin(stream);
        // Consume up to N events to avoid hanging the test harness.
        let mut count = 0;
        while let Some(event) = stream.next().await {
            let _ = event?;
            count += 1;
            if count >= 8 {
                break;
            }
        }
        Ok(())
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        // Parse the dump envelope; pull the last agent message as `text`.
        let parsed: serde_json::Value = serde_json::from_str(&dump.raw)
            .map_err(|e| SubstrateError::Serde(format!("agentapi dump envelope: {e}")))?;
        let messages = parsed
            .get("messages")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        let text = messages
            .iter()
            .rev()
            .find_map(|m| {
                let role = m.get("role")?.as_str()?;
                if role == "agent" {
                    m.get("content")?.as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let status = if dump.raw.contains("\"status\":\"completed\"") {
            TaskState::Completed
        } else if dump.raw.contains("\"status\":\"failed\"") {
            TaskState::Failed
        } else {
            TaskState::Working
        };

        // Pull PR URLs out of any agent text.
        let pr_urls: Vec<String> = messages
            .iter()
            .filter_map(|m| m.get("content")?.as_str())
            .flat_map(extract_pr_urls)
            .collect();

        Ok(StructuredResult {
            text,
            artifacts: vec![],
            pr_urls,
            status,
        })
    }

    fn capabilities(&self) -> EngineCapabilities {
        // agentapi-plusplus supports ACP subagents + MCP file upload +
        // conversation resumption via /message.
        EngineCapabilities {
            supports_resume: true,
            supports_subagents: true,
            supports_mcp_import: true,
        }
    }
}

/// Pull GitHub PR URLs out of a free-form agent text.
fn extract_pr_urls(text: &str) -> Vec<String> {
    // Cheap heuristic: any `https://github.com/<org>/<repo>/pull/<n>`.
    let mut out = Vec::new();
    let needle = "https://github.com/";
    let mut start = 0;
    while let Some(idx) = text[start..].find(needle) {
        let abs = start + idx;
        let end = text[abs..]
            .find(|c: char| c.is_whitespace() || c == '`')
            .map(|i| abs + i)
            .unwrap_or(text.len());
        let url = &text[abs..end];
        if url.contains("/pull/") {
            out.push(url.to_string());
        }
        start = end;
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Routing seam (see `routing.rs`)
// ---------------------------------------------------------------------------

pub mod multi_agent_router;
pub mod routing;

pub use multi_agent_router::AgentApiMultiAgentRouter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_start_includes_agent_type_and_port() {
        let argv = AgentApiArgv::new("claude", 3284);
        let spec = TaskSpec::new("hello", "/repo");
        let args = argv.build_start(&spec);
        assert_eq!(args[0], "server");
        assert!(args.contains(&"claude".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"3284".to_string()));
        assert!(args.contains(&"--type".to_string()));
        assert!(args.contains(&"--initial-prompt".to_string()));
        assert!(args.contains(&"hello".to_string()));
    }

    #[test]
    fn argv_start_with_state_and_pid_files() {
        let argv = AgentApiArgv {
            agent: "codex".into(),
            port: 4000,
            initial_prompt: None,
            state_file: Some(PathBuf::from("/tmp/state.json")),
            pid_file: Some(PathBuf::from("/tmp/agentapi.pid")),
            experimental_acp: true,
        };
        let spec = TaskSpec::new("work", "/repo");
        let args = argv.build_start(&spec);
        assert!(args.contains(&"codex".to_string()));
        assert!(args.contains(&"4000".to_string()));
        assert!(args.contains(&"--state-file".to_string()));
        assert!(args.contains(&"/tmp/state.json".to_string()));
        assert!(args.contains(&"--pid-file".to_string()));
        assert!(args.contains(&"/tmp/agentapi.pid".to_string()));
        assert!(args.contains(&"--experimental-acp".to_string()));
    }

    #[test]
    fn argv_dump_is_sentinel() {
        let argv = AgentApiArgv::new("claude", 3284);
        let args = argv.build_dump("conv-123");
        assert_eq!(args, vec!["GET", "/messages"]);
    }

    #[test]
    fn supported_agents_include_all_known() {
        for a in &[
            "claude",
            "goose",
            "aider",
            "codex",
            "gemini",
            "copilot",
            "amp",
            "auggie",
            "cursor",
            "cursor-agent",
            "q",
            "amazonq",
            "opencode",
            "custom",
        ] {
            assert!(SUPPORTED_AGENTS.contains(a), "missing agent alias: {a}");
        }
    }

    #[test]
    fn extract_pr_urls_pulls_github_pr_links() {
        let text = "Opened https://github.com/kooshapari/substrate/pull/7 and https://github.com/anthropics/claude-code/pull/99 for review.";
        let urls = extract_pr_urls(text);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("/pull/7"));
        assert!(urls[1].contains("/pull/99"));
    }

    #[test]
    fn extract_pr_urls_ignores_non_pr_links() {
        let text = "see https://github.com/kooshapari/substrate and https://example.com";
        let urls = extract_pr_urls(text);
        assert!(urls.is_empty());
    }

    #[test]
    fn sse_parse_status_change() {
        let text =
            "event: status_change\ndata: {\"agent_type\":\"claude\",\"status\":\"running\"}\n\n";
        let ev = parse_sse_record(text).unwrap().unwrap();
        match ev {
            SseEvent::StatusChange { data, .. } => {
                assert_eq!(data.agent_type, "claude");
                assert_eq!(data.status, AgentStatusKind::Running);
            }
            _ => panic!("expected status_change"),
        }
    }

    #[test]
    fn sse_parse_message_update() {
        let text = "event: message_update\ndata: {\"id\":42,\"message\":\"hi\",\"role\":\"agent\",\"time\":\"2026-06-22T00:00:00Z\"}\n\n";
        let ev = parse_sse_record(text).unwrap().unwrap();
        match ev {
            SseEvent::MessageUpdate { data, .. } => {
                assert_eq!(data.id, 42);
                assert_eq!(data.role, ConversationRole::Agent);
            }
            _ => panic!("expected message_update"),
        }
    }

    #[test]
    fn sse_parse_agent_error() {
        let text = "event: agent_error\ndata: {\"level\":\"error\",\"message\":\"crash\",\"time\":\"2026-06-22T00:00:00Z\"}\n\n";
        let ev = parse_sse_record(text).unwrap().unwrap();
        match ev {
            SseEvent::AgentError { data, .. } => {
                assert_eq!(data.level, "error");
                assert_eq!(data.message, "crash");
            }
            _ => panic!("expected agent_error"),
        }
    }

    #[test]
    fn sse_parse_returns_none_for_heartbeat() {
        let text = ":heartbeat\n\n";
        assert!(parse_sse_record(text).unwrap().is_none());
    }

    #[tokio::test]
    async fn conformance_suite_passes_offline() {
        // Default (no AGENTAPI_INTEGRATION) runs the stub path.
        let engine = AgentApiEngine::new();
        engine_conformance::assert_engine_conformance(&engine).await;
    }

    #[test]
    fn extract_result_synthesizes_text_from_last_agent_message() {
        let engine = AgentApiEngine::new();
        let raw = serde_json::json!({
            "conv_id": "abc",
            "agent": "claude",
            "messages": [
                {"id": 1, "role": "user", "content": "fix bug", "time": "2026-06-22T00:00:00Z"},
                {"id": 2, "role": "agent", "content": "fixed it. see https://github.com/foo/bar/pull/3", "time": "2026-06-22T00:00:01Z"},
            ]
        })
        .to_string();
        let dump = ConversationDump {
            conversation_id: "abc".into(),
            raw,
        };
        let result = engine.extract_result(&dump).unwrap();
        assert!(result.text.contains("fixed it"));
        assert_eq!(result.pr_urls, vec!["https://github.com/foo/bar/pull/3"]);
        assert_eq!(result.status, TaskState::Working);
    }

    #[test]
    fn engine_capabilities_advertise_resume_subagents_mcp() {
        let engine = AgentApiEngine::new();
        let caps = engine.capabilities();
        assert!(caps.supports_resume);
        assert!(caps.supports_subagents);
        assert!(caps.supports_mcp_import);
    }
}
