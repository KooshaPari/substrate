//! # gateway
//!
//! OpenAI-compatible HTTP inbound adapter: `/v1/chat/completions`, `/v1/models`,
//! `/a2a/*` mailbox surface, and `/management/config` backed by `store-sqlite`.
#![forbid(unsafe_code)]

pub mod bounded_body;
pub mod circuit_breaker;
mod config;
mod openai;
pub mod streaming;
pub mod upstream;

pub use bounded_body::BoundedBodyConfig;
pub use circuit_breaker::CircuitBreaker;
pub use config::GatewayConfig;
pub use upstream::UpstreamClient;

use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use omniroute_adapter::OmniRouteAdapter;
use serde::{Deserialize, Serialize};
use store_sqlite::{ConfigEntry, SqliteConfigStore, SqliteMailboxStore};
use substrate_core::domain::Task;
use substrate_core::mailbox_port::MailboxStore;
use substrate_core::ports::RoutingPort;

use crate::bounded_body::bounded_body_middleware;
use openai::models_from_decision;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Shared application state wired at the composition root.
#[derive(Clone)]
pub struct AppState {
    routing: Arc<dyn RoutingPort>,
    mailbox: Arc<SqliteMailboxStore>,
    config: Arc<SqliteConfigStore>,
    auth_token: Option<String>,
    upstream: Option<Arc<UpstreamClient>>,
}

impl AppState {
    /// Wire concrete adapters under `state_dir` (creates subdirs as needed).
    pub fn new(state_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let routing: Arc<dyn RoutingPort> = Arc::new(OmniRouteAdapter::new());
        let mailbox_db = state_dir.join("mailbox.db");
        let config_db = state_dir.join("gateway.db");
        let mailbox =
            Arc::new(SqliteMailboxStore::open(mailbox_db.to_str().ok_or_else(
                || anyhow::anyhow!("state path is not valid UTF-8"),
            )?)?);
        let config =
            Arc::new(SqliteConfigStore::open(config_db.to_str().ok_or_else(
                || anyhow::anyhow!("state path is not valid UTF-8"),
            )?)?);
        Ok(Self {
            routing,
            mailbox,
            config,
            auth_token: None,
            upstream: None,
        })
    }

    /// Attach an optional bearer token for protected routes.
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    /// Attach the upstream OpenAI-compatible provider client.
    pub fn with_upstream(mut self, upstream: UpstreamClient) -> Self {
        self.upstream = Some(Arc::new(upstream));
        self
    }
}

/// Build an in-memory test state with an injected [`RoutingPort`].
#[doc(hidden)]
pub fn test_state(state_dir: &Path, routing: Arc<dyn RoutingPort>) -> anyhow::Result<AppState> {
    std::fs::create_dir_all(state_dir)?;
    let mailbox = Arc::new(SqliteMailboxStore::open_in_memory()?);
    let config = Arc::new(SqliteConfigStore::open_in_memory()?);
    Ok(AppState {
        routing,
        mailbox,
        config,
        auth_token: None,
        upstream: None,
    })
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the axum router for `state`.
pub fn build_router(state: AppState) -> Router {
    let chat = Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .layer(middleware::from_fn_with_state(
            BoundedBodyConfig::default(),
            bounded_body_middleware,
        ));
    let protected = Router::new()
        .merge(chat)
        .route("/v1/models", get(models_handler))
        .route("/management/config", post(management_config_handler))
        .route("/a2a/messages", post(a2a_send_handler))
        .route("/a2a/inbox", get(a2a_inbox_handler))
        .route("/a2a/tasks", get(a2a_tasks_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/healthz", get(healthz_handler))
        .merge(protected)
        .with_state(state)
}

/// Bind and serve the gateway using `config`.
pub async fn serve(config: GatewayConfig) -> anyhow::Result<()> {
    let state = AppState::new(&config.state_dir)?
        .with_auth_token(config.auth_token)
        .with_upstream(UpstreamClient::new(
            config.upstream_url,
            config.upstream_key,
        ));
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

async fn require_auth(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(expected) = &state.auth_token {
        let authorized = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|h| h == format!("Bearer {expected}"));
        if !authorized {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------------
// OpenAI handlers
// ---------------------------------------------------------------------------

async fn healthz_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn models_handler(
    State(state): State<AppState>,
) -> Result<Json<openai::ModelsResponse>, ApiError> {
    let task = Task::new("models", ".");
    let decision = state
        .routing
        .route_decision(&task)
        .await
        .map_err(|e| ApiError::internal(format!("route failed: {e}")))?;
    Ok(Json(models_from_decision(&decision)))
}

async fn chat_completions_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    let upstream = state
        .upstream
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("upstream provider not configured"))?;
    upstream
        .chat_completions(body)
        .await
        .map_err(ApiError::from_upstream)
}

// ---------------------------------------------------------------------------
// Management config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ConfigRequest {
    action: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConfigEntryResponse {
    entry: ConfigEntry,
}

#[derive(Debug, Serialize)]
struct ConfigListResponse {
    entries: Vec<ConfigEntry>,
}

#[derive(Debug, Serialize)]
struct ConfigDeleteResponse {
    deleted: bool,
}

async fn management_config_handler(
    State(state): State<AppState>,
    Json(body): Json<ConfigRequest>,
) -> Result<Response, ApiError> {
    match body.action.as_str() {
        "set" => {
            let key = body
                .key
                .filter(|k| !k.trim().is_empty())
                .ok_or_else(|| ApiError::bad_request("key is required for set"))?;
            let value = body
                .value
                .ok_or_else(|| ApiError::bad_request("value is required for set"))?;
            let entry = state
                .config
                .set(&key, &value)
                .map_err(|e| ApiError::internal(format!("config set failed: {e}")))?;
            Ok(Json(ConfigEntryResponse { entry }).into_response())
        }
        "get" => {
            let key = body
                .key
                .filter(|k| !k.trim().is_empty())
                .ok_or_else(|| ApiError::bad_request("key is required for get"))?;
            let entry = state
                .config
                .get(&key)
                .map_err(|e| ApiError::internal(format!("config get failed: {e}")))?
                .ok_or_else(|| ApiError::not_found(format!("config key not found: {key}")))?;
            Ok(Json(ConfigEntryResponse { entry }).into_response())
        }
        "list" => {
            let entries = state
                .config
                .list()
                .map_err(|e| ApiError::internal(format!("config list failed: {e}")))?;
            Ok(Json(ConfigListResponse { entries }).into_response())
        }
        "delete" => {
            let key = body
                .key
                .filter(|k| !k.trim().is_empty())
                .ok_or_else(|| ApiError::bad_request("key is required for delete"))?;
            let deleted = state
                .config
                .delete(&key)
                .map_err(|e| ApiError::internal(format!("config delete failed: {e}")))?;
            Ok(Json(ConfigDeleteResponse { deleted }).into_response())
        }
        other => Err(ApiError::bad_request(format!(
            "unknown action: {other}; use set, get, list, or delete"
        ))),
    }
}

// ---------------------------------------------------------------------------
// A2A handlers
// ---------------------------------------------------------------------------

async fn a2a_send_handler(
    State(state): State<AppState>,
    Json(msg): Json<a2a::Message>,
) -> Result<StatusCode, ApiError> {
    state
        .mailbox
        .post(&msg)
        .map_err(|e| ApiError::internal(format!("mailbox post failed: {e}")))?;
    Ok(StatusCode::CREATED)
}

#[derive(Debug, Deserialize)]
struct InboxQuery {
    team: String,
    to: String,
}

async fn a2a_inbox_handler(
    State(state): State<AppState>,
    Query(query): Query<InboxQuery>,
) -> Result<Json<Vec<a2a::Message>>, ApiError> {
    if query.team.trim().is_empty() || query.to.trim().is_empty() {
        return Err(ApiError::bad_request(
            "team and to query params are required",
        ));
    }
    let msgs = state
        .mailbox
        .inbox(&query.team, &query.to)
        .map_err(|e| ApiError::internal(format!("mailbox inbox failed: {e}")))?;
    Ok(Json(msgs))
}

#[derive(Debug, Deserialize)]
struct TasksQuery {
    team: String,
}

async fn a2a_tasks_handler(
    State(state): State<AppState>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<Vec<a2a::Task>>, ApiError> {
    if query.team.trim().is_empty() {
        return Err(ApiError::bad_request("team query param is required"));
    }
    let tasks = state
        .mailbox
        .task_list(&query.team)
        .map_err(|e| ApiError::internal(format!("task list failed: {e}")))?;
    Ok(Json(tasks))
}

// ---------------------------------------------------------------------------
// API errors
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }

    fn service_unavailable(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: msg.into(),
        }
    }

    fn bad_gateway(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: msg.into(),
        }
    }

    fn from_upstream(err: crate::upstream::UpstreamError) -> Self {
        match err {
            crate::upstream::UpstreamError::CircuitOpen => {
                Self::service_unavailable("upstream circuit breaker open")
            }
            crate::upstream::UpstreamError::BadRequest(msg) => Self::bad_request(msg),
            crate::upstream::UpstreamError::Request(err) => {
                Self::bad_gateway(format!("upstream request failed: {err}"))
            }
            crate::upstream::UpstreamError::BuildResponse(msg) => {
                Self::internal(format!("upstream response build failed: {msg}"))
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}
