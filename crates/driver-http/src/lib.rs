//! # driver-http
//!
//! HTTP/REST inbound adapter exposing substrate dispatch, planning, routing,
//! and A2A mailbox APIs for non-Rust consumers (Go agentapi-plusplus, TS OmniRoute).
#![forbid(unsafe_code)]

mod config;
mod plan;

pub use config::HttpConfig;

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
use engine_forge::ForgeEngine;
use engine_spec::TaskSpec;
use plan::{engine_catalog, enrich_plan_argv};
use routing_phenotype_router::PhenotypeRouterAdapter;
use serde::{Deserialize, Serialize};
use store_file::FileStore;
use store_sqlite::SqliteMailboxStore;
use substrate_app::{DispatchPlanner, DispatchService, PlanRequest, SessionMode};
use substrate_core::domain::{RoutingDecision, StructuredResult, Task};
use substrate_core::mailbox_port::MailboxStore;
use substrate_core::ports::{DispatchApi, RoutingPort};
use transport_file::FileTransport;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Shared application state wired at the composition root.
#[derive(Clone)]
pub struct AppState {
    dispatch: Arc<dyn DispatchApi>,
    routing: Arc<dyn RoutingPort>,
    mailbox: Arc<SqliteMailboxStore>,
    auth_token: Option<String>,
}

impl AppState {
    /// Wire concrete adapters under `state_dir` (creates subdirs as needed).
    pub fn new(state_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let store = Arc::new(FileStore::new(state_dir.join("store"))?);
        let transport = Arc::new(FileTransport::new(state_dir.join("mailbox"))?);
        let forge = Arc::new(ForgeEngine::new());
        let dispatch: Arc<dyn DispatchApi> =
            Arc::new(DispatchService::new(forge, store, transport));
        let routing: Arc<dyn RoutingPort> = Arc::new(PhenotypeRouterAdapter::new());
        let mailbox_db = state_dir.join("mailbox.db");
        let mailbox =
            Arc::new(SqliteMailboxStore::open(mailbox_db.to_str().ok_or_else(
                || anyhow::anyhow!("state path is not valid UTF-8"),
            )?)?);
        Ok(Self {
            dispatch,
            routing,
            mailbox,
            auth_token: None,
        })
    }

    /// Attach an optional bearer token for protected routes.
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }
}

/// Build an in-memory test state (offline; optional explicit forge binary path).
#[doc(hidden)]
pub fn test_state(state_dir: &Path, forge_bin: Option<&Path>) -> anyhow::Result<AppState> {
    std::fs::create_dir_all(state_dir)?;
    let store = Arc::new(FileStore::new(state_dir.join("store"))?);
    let transport = Arc::new(FileTransport::new(state_dir.join("mailbox"))?);
    let forge: Arc<ForgeEngine> = match forge_bin {
        Some(path) => Arc::new(ForgeEngine::with_bin(path.to_string_lossy())),
        None => Arc::new(ForgeEngine::new()),
    };
    let dispatch: Arc<dyn DispatchApi> = Arc::new(DispatchService::new(forge, store, transport));
    let routing: Arc<dyn RoutingPort> = Arc::new(PhenotypeRouterAdapter::new());
    let mailbox = Arc::new(SqliteMailboxStore::open_in_memory()?);
    Ok(AppState {
        dispatch,
        routing,
        mailbox,
        auth_token: None,
    })
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the axum router for `state`.
pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/dispatch", post(dispatch_handler))
        .route("/v1/plan", post(plan_handler))
        .route("/v1/route", post(route_handler))
        .route("/v1/mailbox/send", post(mailbox_send_handler))
        .route("/v1/mailbox/inbox", get(mailbox_inbox_handler))
        .route("/v1/tasks", get(tasks_list_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/healthz", get(healthz_handler))
        .merge(protected)
        .with_state(state)
}

/// Bind and serve the HTTP API using `config`.
pub async fn serve(config: HttpConfig) -> anyhow::Result<()> {
    let state = AppState::new(&config.state_dir)?.with_auth_token(config.auth_token);
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
// Handlers
// ---------------------------------------------------------------------------

async fn healthz_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

#[derive(Debug, Deserialize)]
struct PromptBody {
    #[serde(default)]
    engine: Option<String>,
    cwd: String,
    prompt: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    resume: Option<String>,
}

impl PromptBody {
    fn validate(&self) -> Result<(), ApiError> {
        if self.cwd.trim().is_empty() {
            return Err(ApiError::bad_request("cwd must not be empty"));
        }
        if self.prompt.trim().is_empty() {
            return Err(ApiError::bad_request("prompt must not be empty"));
        }
        Ok(())
    }

    fn task_spec(&self) -> TaskSpec {
        let mut spec = TaskSpec::new(&self.prompt, &self.cwd);
        if let Some(agent) = &self.agent {
            spec = spec.with_agent(agent.clone());
        }
        if let Some(resume) = &self.resume {
            spec.resume = Some(resume.clone());
        }
        spec
    }

    fn session_mode(&self) -> Result<Option<SessionMode>, ApiError> {
        match &self.mode {
            None => Ok(None),
            Some(s) => SessionMode::parse_cli(s).map(Some).ok_or_else(|| {
                ApiError::bad_request("invalid mode: use background, foreground, or in_process")
            }),
        }
    }

    fn plan(&self) -> Result<substrate_app::DispatchPlan, ApiError> {
        let spec = self.task_spec();
        let engines = engine_catalog();
        let mut plan = DispatchPlanner::plan(&PlanRequest {
            spec: &spec,
            engines: &engines,
            explicit_engine: self.engine.as_deref(),
            session_mode: self.session_mode()?,
            routing_engine: self.engine.as_deref().or(Some("forge")),
        })
        .map_err(|e| ApiError::unprocessable(format!("plan failed: {e}")))?;
        enrich_plan_argv(&mut plan);
        Ok(plan)
    }
}

async fn plan_handler(
    State(_state): State<AppState>,
    Json(body): Json<PromptBody>,
) -> Result<Json<substrate_app::DispatchPlan>, ApiError> {
    body.validate()?;
    Ok(Json(body.plan()?))
}

async fn dispatch_handler(
    State(state): State<AppState>,
    Json(body): Json<PromptBody>,
) -> Result<Json<StructuredResult>, ApiError> {
    body.validate()?;
    let plan = body.plan()?;
    if plan.engine != "forge" {
        return Err(ApiError::unprocessable(format!(
            "execution wiring supports forge only in this build; plan selected {}",
            plan.engine
        )));
    }
    let task = Task::new(plan.spec.prompt.clone(), plan.spec.cwd.clone());
    let result = state
        .dispatch
        .dispatch(task)
        .await
        .map_err(|e| ApiError::internal(format!("dispatch failed: {e}")))?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct RouteBody {
    task: Task,
}

async fn route_handler(
    State(state): State<AppState>,
    Json(body): Json<RouteBody>,
) -> Result<Json<RoutingDecision>, ApiError> {
    let decision = state
        .routing
        .route_decision(&body.task)
        .await
        .map_err(|e| ApiError::internal(format!("route failed: {e}")))?;
    Ok(Json(decision))
}

async fn mailbox_send_handler(
    State(state): State<AppState>,
    Json(msg): Json<psub_a2a::Message>,
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

async fn mailbox_inbox_handler(
    State(state): State<AppState>,
    Query(query): Query<InboxQuery>,
) -> Result<Json<Vec<psub_a2a::Message>>, ApiError> {
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

async fn tasks_list_handler(
    State(state): State<AppState>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<Vec<psub_a2a::Task>>, ApiError> {
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

    fn unprocessable(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: msg.into(),
        }
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
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
