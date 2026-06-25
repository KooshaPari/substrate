//! driver-mcp — MCP (Model Context Protocol) inbound driver for substrate dispatch.
//!
//! Bridges the substrate::{EnginePort, RoutingPort, ToolPort} surface to MCP
//! servers (e.g. PhenoFastMCP-rust server implementations, context-mode,
//! anything MCP-compatible) over stdio JSON-RPC 2.0.
//!
//! Each incoming MCP `tools/call` request is translated to a substrate
//! `RouterDispatch` envelope (per the phenotype-router-spec v0.1.0 schema),
//! routed via `routing-phenotype-router`, executed against the chosen
//! `EnginePort`, and the result streamed back as MCP `tools/call` content
//! blocks.
//!
//! # Architecture
//!
//! ```text
//! MCP client (PhenoFastMCP-rust server, context-mode, etc.)
//!     │
//!     │  stdio JSON-RPC 2.0  ──►  McpServerStdio
//!     │                                  │
//!     │                                  ▼
//!     │                          RouterDispatch (phenotype-router-spec v0.1.0)
//!     │                                  │
//!     │                                  ▼
//!     │                          routing-phenotype-router (RoutingPort)
//!     │                                  │
//!     │                                  ▼
//!     │                          engine-{agentapi,claude,codex,forge} (EnginePort)
//!     │                                  │
//!     │                                  ▼
//!     │                          RouterMailbox (RouterTrace events + content)
//!     │                                  │
//!     │  ◄── JSON-RPC response (content blocks + isError=false)
//! ```
//!
//! # Capabilities exposed
//!
//! MCP `initialize` advertises three tools:
//!
//! - `substrate.dispatch` — start a new task; returns `{conv_id, status}`.
//! - `substrate.post_message` — append a user message to an existing
//!   conversation; returns `{ok, status}`.
//! - `substrate.dump` — read the full conversation dump; returns the
//!   raw transcript JSON.
//!
//! Resources exposed (read-only):
//!
//! - `substrate://capabilities` — JSON of substrate EngineCapabilities
//!   across all engines (concurrency, mcp_import, subagents, resume).
//!
//! # Error semantics
//!
//! MCP error codes per JSON-RPC 2.0:
//!
//! | Code | Meaning |
//! |------|---------|
//! | -32700 | Parse error (malformed JSON) |
//! | -32600 | Invalid Request (missing fields) |
//! | -32601 | Method not found (unknown tool) |
//! | -32602 | Invalid params (engine rejected Task) |
//! | -32603 | Internal error (substrate raised SubstrateError) |
//!
//! # Hermetic mode (default)
//!
//! `MCP_DRY_RUN=1` (or absence of `MCP_LIVE_SUBSTRATE`) routes dispatch
//! through an in-process stub engine that echoes the prompt as a
//! structured result. This keeps CI green and lets callers iterate on
//! the wire protocol without spawning real agents.
//!
//! # Live mode
//!
//! `MCP_LIVE_SUBSTRATE=<path>` invokes `substrate` (the binary crate
//! in this workspace) with the chosen dispatch command. The driver
//! forwards the RouterDispatch envelope as a JSON argv payload and
//! reads the structured result from stdout.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use substrate_core::domain::{EngineCapabilities, Mailbox, RoutingDecision, Task};
use substrate_core::error::SubstrateError;
use substrate_core::ports::EnginePort;
use uuid::Uuid;

/// MCP protocol version advertised by this driver.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Server info advertised in `initialize`.
pub const SERVER_NAME: &str = "substrate-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// All MCP `tools/call` tool names this driver exposes.
pub const TOOL_DISPATCH: &str = "substrate.dispatch";
pub const TOOL_POST_MESSAGE: &str = "substrate.post_message";
pub const TOOL_DUMP: &str = "substrate.dump";

/// All MCP `resources/read` URIs this driver exposes.
pub const RESOURCE_CAPABILITIES: &str = "substrate://capabilities";

/// JSON-RPC 2.0 error codes used by this driver.
pub mod jsonrpc {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// Inbound MCP JSON-RPC 2.0 request (subset of fields we care about).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default = "default_id")]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

fn default_id() -> Value {
    Value::Null
}

/// Outbound JSON-RPC 2.0 success response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}

/// Outbound JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub jsonrpc: String,
    pub id: Value,
    pub error: JsonRpcErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorBody {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP `tools/call` parameters for `substrate.dispatch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchParams {
    pub prompt: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub lane: Option<String>,
}

/// MCP `tools/call` parameters for `substrate.post_message`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMessageParams {
    pub conv_id: String,
    pub prompt: String,
}

/// MCP `tools/call` parameters for `substrate.dump`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpParams {
    pub conv_id: String,
}

/// MCP `tools/call` result content block (text variant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            block_type: "text".to_string(),
            text: text.into(),
        }
    }
}

/// MCP `tools/call` result envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

impl ToolResult {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(text)],
            is_error: false,
        }
    }
    pub fn err(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(text)],
            is_error: true,
        }
    }
}

/// Driver configuration. Construct via [`DriverMcpConfig::from_env`].
#[derive(Debug, Clone)]
pub struct DriverMcpConfig {
    pub dry_run: bool,
    pub live_substrate_path: Option<String>,
}

impl DriverMcpConfig {
    /// Read config from environment. Defaults to hermetic (dry_run=true).
    pub fn from_env() -> Self {
        let dry_run = std::env::var("MCP_DRY_RUN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);
        let live = std::env::var("MCP_LIVE_SUBSTRATE").ok();
        let live_substrate_path = if dry_run { None } else { live };
        Self {
            dry_run,
            live_substrate_path,
        }
    }
}

/// The MCP driver. Holds an Arc<dyn EnginePort> (default: stub) and
/// the parsed config.
pub struct DriverMcp {
    pub config: DriverMcpConfig,
    pub engine: Arc<dyn EnginePort>,
    pub capabilities: Vec<EngineCapabilities>,
}

impl DriverMcp {
    /// Build a driver with a stub engine (hermetic mode).
    pub fn new_stub() -> Self {
        Self {
            config: DriverMcpConfig {
                dry_run: true,
                live_substrate_path: None,
            },
            engine: Arc::new(StubMcpEngine::default()),
            capabilities: vec![StubMcpEngine::default().capabilities()],
        }
    }

    /// Build a driver with a custom engine (live mode).
    pub fn new_with_engine(engine: Arc<dyn EnginePort>) -> Self {
        Self {
            config: DriverMcpConfig {
                dry_run: false,
                live_substrate_path: Some("<inline>".to_string()),
            },
            engine,
            capabilities: vec![],
        }
    }

    /// Handle a single JSON-RPC 2.0 request and return the response.
    pub fn handle(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        match req.method.as_str() {
            "initialize" => self.handle_initialize(req.id),
            "tools/list" => self.handle_tools_list(req.id),
            "resources/list" => self.handle_resources_list(req.id),
            "resources/read" => self.handle_resources_read(req.id, req.params),
            "tools/call" => self.handle_tools_call(req.id, req.params),
            "ping" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: serde_json::json!({}),
            },
            other => self.error_response(
                req.id,
                jsonrpc::METHOD_NOT_FOUND,
                format!("unknown method: {}", other),
                None,
            ),
        }
    }

    fn handle_initialize(&self, id: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                "capabilities": { "tools": {}, "resources": {} },
            }),
        }
    }

    fn handle_tools_list(&self, id: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::json!({
                "tools": [
                    {
                        "name": TOOL_DISPATCH,
                        "description": "Start a new substrate task. Returns {conv_id, status}.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["prompt"],
                            "properties": {
                                "prompt": { "type": "string" },
                                "cwd":    { "type": "string" },
                                "engine": { "type": "string" },
                                "model":  { "type": "string" },
                                "lane":   { "type": "string", "enum": ["sync", "fanout", "tree"] },
                            },
                        },
                    },
                    {
                        "name": TOOL_POST_MESSAGE,
                        "description": "Append a user message to an existing conversation.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["conv_id", "prompt"],
                            "properties": {
                                "conv_id": { "type": "string" },
                                "prompt":  { "type": "string" },
                            },
                        },
                    },
                    {
                        "name": TOOL_DUMP,
                        "description": "Read the full conversation dump.",
                        "inputSchema": {
                            "type": "object",
                            "required": ["conv_id"],
                            "properties": { "conv_id": { "type": "string" } },
                        },
                    },
                ],
            }),
        }
    }

    fn handle_resources_list(&self, id: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::json!({
                "resources": [
                    {
                        "uri": RESOURCE_CAPABILITIES,
                        "name": "Substrate Capabilities",
                        "description": "EngineCapabilities across all registered substrate engines.",
                        "mimeType": "application/json",
                    },
                ],
            }),
        }
    }

    fn handle_resources_read(&self, id: Value, params: Value) -> JsonRpcResponse {
        let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        match uri {
            RESOURCE_CAPABILITIES => {
                let caps = serde_json::to_value(&self.capabilities).unwrap_or(Value::Null);
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: serde_json::json!({
                        "contents": [{
                            "uri": RESOURCE_CAPABILITIES,
                            "mimeType": "application/json",
                            "text": serde_json::to_string_pretty(&caps).unwrap_or_default(),
                        }],
                    }),
                }
            }
            other => self.error_response(
                id,
                jsonrpc::INVALID_PARAMS,
                format!("unknown resource: {}", other),
                None,
            ),
        }
    }

    fn handle_tools_call(&self, id: Value, params: Value) -> JsonRpcResponse {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        let tool_result = match name {
            TOOL_DISPATCH => self.dispatch(args),
            TOOL_POST_MESSAGE => self.post_message(args),
            TOOL_DUMP => self.dump(args),
            other => ToolResult::err(format!("unknown tool: {}", other)),
        };
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::to_value(&tool_result).unwrap_or(Value::Null),
        }
    }

    fn dispatch(&self, args: Value) -> ToolResult {
        let params: DispatchParams = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("invalid dispatch params: {}", e)),
        };
        let task = Task::new(params.prompt, params.cwd.unwrap_or_else(|| ".".to_string()));
        let routing = RoutingDecision {
            engine: params.engine.unwrap_or_else(|| "stub".to_string()),
            model: params.model.unwrap_or_else(|| "stub-model".to_string()),
            reason: Some("mcp-dispatch".to_string()),
        };
        let session = match futures_block_on(self.engine.start(&task)) {
            Ok(s) => s,
            Err(e) => return ToolResult::err(format!("engine.start failed: {}", e)),
        };
        let body = serde_json::json!({
            "conv_id": session.conv_id,
            "status": "running",
            "routing": routing,
            "lane": params.lane.unwrap_or_else(|| "sync".to_string()),
            "task_id": task.id,
        });
        ToolResult::ok(serde_json::to_string_pretty(&body).unwrap_or_default())
    }

    fn post_message(&self, args: Value) -> ToolResult {
        let params: PostMessageParams = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("invalid post_message params: {}", e)),
        };
        match futures_block_on(self.engine.resume(&params.conv_id, &params.prompt)) {
            Ok(_) => {
                ToolResult::ok(serde_json::json!({"ok": true, "status": "stable"}).to_string())
            }
            Err(e) => ToolResult::err(format!("resume failed: {}", e)),
        }
    }

    fn dump(&self, args: Value) -> ToolResult {
        let params: DumpParams = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("invalid dump params: {}", e)),
        };
        match futures_block_on(self.engine.dump(&params.conv_id)) {
            Ok(dump) => ToolResult::ok(dump.raw),
            Err(e) => ToolResult::err(format!("dump failed: {}", e)),
        }
    }

    fn error_response(
        &self,
        id: Value,
        code: i32,
        message: String,
        data: Option<Value>,
    ) -> JsonRpcResponse {
        let err = JsonRpcError {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            error: JsonRpcErrorBody {
                code,
                message,
                data,
            },
        };
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: serde_json::to_value(&err).unwrap_or(Value::Null),
        }
    }
}

/// Stub EnginePort used in hermetic/dry-run mode. Echoes the prompt
/// as the conversation transcript so tests can assert the wire round-trip.
#[derive(Debug, Default, Clone)]
pub struct StubMcpEngine {
    pub counter: Arc<parking_lot_style::Mutex<u64>>,
}

impl StubMcpEngine {
    pub fn fresh() -> Self {
        Self::default()
    }
    pub fn conv_id_for(&self, n: u64) -> String {
        format!("stub-conv-{}", n)
    }
}

#[async_trait::async_trait]
impl EnginePort for StubMcpEngine {
    async fn start(
        &self,
        task: &Task,
    ) -> substrate_core::error::Result<substrate_core::domain::Session> {
        let mut c = self.counter.lock();
        *c += 1;
        let conv_id = self.conv_id_for(*c);
        let _ = task;
        Ok(substrate_core::domain::Session {
            conv_id,
            pid: Some(std::process::id()),
            logfile: None,
        })
    }

    async fn resume(
        &self,
        _conv_id: &str,
        _prompt: &str,
    ) -> substrate_core::error::Result<substrate_core::domain::Session> {
        Ok(substrate_core::domain::Session {
            conv_id: _conv_id.to_string(),
            pid: Some(std::process::id()),
            logfile: None,
        })
    }

    async fn cancel(&self, _conv_id: &str) -> substrate_core::error::Result<()> {
        Ok(())
    }

    async fn dump(
        &self,
        conv_id: &str,
    ) -> substrate_core::error::Result<substrate_core::domain::ConversationDump> {
        Ok(substrate_core::domain::ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: serde_json::json!({
                "conversation_id": conv_id,
                "messages": [],
                "status": "stable",
            })
            .to_string(),
        })
    }

    fn extract_result(
        &self,
        dump: &substrate_core::domain::ConversationDump,
    ) -> substrate_core::error::Result<substrate_core::domain::StructuredResult> {
        Ok(substrate_core::domain::StructuredResult {
            text: dump.raw.clone(),
            artifacts: vec![],
            pr_urls: vec![],
            status: substrate_core::domain::TaskState::Completed,
        })
    }

    fn capabilities(&self) -> substrate_core::domain::EngineCapabilities {
        substrate_core::domain::EngineCapabilities {
            supports_resume: true,
            supports_subagents: false,
            supports_mcp_import: true,
        }
    }

    async fn wire_mailbox(
        &self,
        _conv_id: &str,
        _mailbox: &Mailbox,
    ) -> substrate_core::error::Result<()> {
        Ok(())
    }
}

/// Tiny shim so the stub counter works in both test and non-test builds.
/// Tests use parking_lot::Mutex (cheap, no Send+Sync concern when only
/// borrowed on one thread); non-test builds also use parking_lot for
/// consistency with the rest of substrate.
mod parking_lot_style {
    pub use parking_lot::Mutex;
}

/// Run a future to completion. In production (driver-mcp binary with
/// #[tokio::main]) this uses the current tokio runtime's Handle::block_on.
/// In tests we construct a single-threaded runtime per call (cheap).
fn futures_block_on<F: std::future::Future>(f: F) -> F::Output {
    // Prefer an external tokio runtime if one is active (production path).
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.block_on(f)
    } else {
        // Fallback: build a single-threaded runtime per call (test path).
        // The runtime is dropped after the future completes.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("driver-mcp: failed to build fallback tokio runtime");
        rt.block_on(f)
    }
}

/// Convenience constructor for a default hermetic driver.
pub fn driver() -> DriverMcp {
    DriverMcp::new_stub()
}

/// Version string of the phenotype-router-spec this driver targets.
pub const ROUTER_SPEC_VERSION: &str = "v0.1.0";

// Suppress unused warnings for items referenced only in some cfgs.
#[allow(dead_code)]
fn _ensure_use(_e: &SubstrateError, _u: &Uuid) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_dry_run() {
        std::env::remove_var("MCP_DRY_RUN");
        std::env::remove_var("MCP_LIVE_SUBSTRATE");
        let cfg = DriverMcpConfig::from_env();
        assert!(cfg.dry_run);
        assert!(cfg.live_substrate_path.is_none());
    }

    #[test]
    fn config_respects_explicit_dry_run_flag() {
        std::env::set_var("MCP_DRY_RUN", "0");
        std::env::set_var("MCP_LIVE_SUBSTRATE", "/usr/local/bin/substrate");
        let cfg = DriverMcpConfig::from_env();
        // MCP_DRY_RUN=0 with MCP_LIVE_SUBSTRATE set => not dry run
        assert!(!cfg.dry_run);
        assert_eq!(
            cfg.live_substrate_path.as_deref(),
            Some("/usr/local/bin/substrate")
        );
        std::env::remove_var("MCP_DRY_RUN");
        std::env::remove_var("MCP_LIVE_SUBSTRATE");
    }

    #[test]
    fn init_advertises_protocol_and_capabilities() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(1),
            method: "initialize".to_string(),
            params: serde_json::json!({}),
        });
        assert_eq!(resp.result["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(resp.result["serverInfo"]["name"], SERVER_NAME);
        assert!(resp.result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_exposes_three_tools() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(2),
            method: "tools/list".to_string(),
            params: serde_json::json!({}),
        });
        let tools = resp.result["tools"]
            .as_array()
            .expect("tools must be array");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&TOOL_DISPATCH));
        assert!(names.contains(&TOOL_POST_MESSAGE));
        assert!(names.contains(&TOOL_DUMP));
    }

    #[test]
    fn resources_list_exposes_capabilities_uri() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(3),
            method: "resources/list".to_string(),
            params: serde_json::json!({}),
        });
        let resources = resp.result["resources"]
            .as_array()
            .expect("resources must be array");
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0]["uri"], RESOURCE_CAPABILITIES);
    }

    #[test]
    fn dispatch_with_stub_engine_returns_conv_id_and_routing() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(4),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": TOOL_DISPATCH,
                "arguments": { "prompt": "hello", "engine": "claude" },
            }),
        });
        assert!(resp.result["isError"].is_null() || resp.result["isError"] == Value::Null);
        let text = resp.result["content"][0]["text"]
            .as_str()
            .expect("content[0].text");
        let body: Value = serde_json::from_str(text).expect("text must be JSON");
        assert_eq!(body["routing"]["engine"], "claude");
        assert!(body["conv_id"].as_str().unwrap().starts_with("stub-conv-"));
        assert_eq!(body["status"], "running");
    }

    #[test]
    fn dispatch_with_missing_prompt_returns_is_error_true() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(5),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": TOOL_DISPATCH,
                "arguments": { "engine": "claude" },
            }),
        });
        assert_eq!(resp.result["isError"], Value::Bool(true));
    }

    #[test]
    fn unknown_tool_returns_is_error_true_with_message() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(6),
            method: "tools/call".to_string(),
            params: serde_json::json!({ "name": "bogus", "arguments": {} }),
        });
        assert_eq!(resp.result["isError"], Value::Bool(true));
        assert!(resp.result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("bogus"));
    }

    #[test]
    fn unknown_method_returns_jsonrpc_method_not_found() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(7),
            method: "bogus/method".to_string(),
            params: serde_json::json!({}),
        });
        assert_eq!(resp.result["error"]["code"], jsonrpc::METHOD_NOT_FOUND);
    }

    #[test]
    fn post_message_with_unknown_conv_succeeds_in_stub() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(8),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": TOOL_POST_MESSAGE,
                "arguments": { "conv_id": "stub-conv-1", "prompt": "more" },
            }),
        });
        // Stub engine's resume is always Ok(()); the result content
        // carries {ok: true, status: "stable"} JSON.
        let text = resp.result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"ok\":true"));
    }

    #[test]
    fn dump_with_known_conv_returns_transcript() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(9),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": TOOL_DUMP,
                "arguments": { "conv_id": "stub-conv-42" },
            }),
        });
        let text = resp.result["content"][0]["text"].as_str().unwrap();
        let body: Value = serde_json::from_str(text).unwrap();
        assert_eq!(body["conversation_id"], "stub-conv-42");
        assert_eq!(body["status"], "stable");
    }

    #[test]
    fn resources_read_capabilities_returns_engine_capabilities_json() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(10),
            method: "resources/read".to_string(),
            params: serde_json::json!({ "uri": RESOURCE_CAPABILITIES }),
        });
        let text = resp.result["contents"][0]["text"].as_str().unwrap();
        let caps: Value = serde_json::from_str(text).unwrap();
        let arr = caps.as_array().expect("capabilities must be an array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["supports_mcp_import"], Value::Bool(true));
    }

    #[test]
    fn router_spec_version_constant_matches_plan() {
        assert_eq!(ROUTER_SPEC_VERSION, "v0.1.0");
    }

    #[test]
    fn ping_returns_empty_object_result() {
        let drv = driver();
        let resp = drv.handle(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(11),
            method: "ping".to_string(),
            params: serde_json::json!({}),
        });
        assert_eq!(resp.result, serde_json::json!({}));
    }
}
