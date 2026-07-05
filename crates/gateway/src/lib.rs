//! # gateway
//!
//! OpenAI-compatible HTTP inbound adapter: `/v1/chat/completions`, `/v1/models`,
//! `/a2a/*` mailbox surface, and `/management/config` backed by `store-sqlite`.
#![forbid(unsafe_code)]

pub mod admin;
pub mod audit_log;
pub mod banner;
pub mod bounded_body;
pub mod budget;
pub mod circuit_breaker;
mod config;
pub mod config_watcher;
pub mod fallback;
pub mod metrics;
mod openai;
pub mod rate_limit;
pub mod request_rate;
pub mod retry;
pub mod router;
pub mod streaming;
pub mod upstream;

pub use audit_log::{AuditEntry, AuditLogger};
pub use bounded_body::BoundedBodyConfig;
pub use budget::{BudgetConfig, BudgetStore};
pub use circuit_breaker::CircuitBreaker;
pub use config::{resolve_provider, AuthScheme, GatewayConfig, ProviderConfig};
pub use config_watcher::FileConfig;
pub use fallback::{try_with_fallback, FallbackChain};
pub use metrics::MetricsStore;
pub use rate_limit::{RateLimiterConfig, RateLimiterStore};
pub use router::{ProviderEntry, ProviderRouter};
pub use upstream::UpstreamClient;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{watch, RwLock as TokioRwLock};

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use rate_limit::RateLimitError;
use routing_phenotype_router::PhenotypeRouterAdapter;
use serde::{Deserialize, Serialize};
use store_sqlite::{ConfigEntry, SqliteConfigStore, SqliteMailboxStore};
use substrate_core::domain::Task;
use substrate_core::mailbox_port::MailboxStore;
use substrate_core::ports::RoutingPort;

use openai::{complete_chat, complete_chat_stream, models_from_decision, ChatCompletionRequest};
use streaming::StreamingResponseBuilder;

// ---------------------------------------------------------------------------
// Audit log store
// ---------------------------------------------------------------------------

/// Maximum number of entries retained in the in-memory log ring buffer.
const LOG_RING_CAPACITY: usize = 100;

/// A single audit log entry recording one chat completion request.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    /// RFC 3339 timestamp of the request.
    pub timestamp: String,
    /// Provider name derived from the model field (e.g. `"openai"`).
    pub provider: String,
    /// Full model string as sent by the client (e.g. `"openai/gpt-4"`).
    pub model: String,
    /// HTTP status code returned to the client.
    pub status_code: u16,
    /// Request round-trip latency in milliseconds.
    pub latency_ms: u64,
}

/// Thread-safe ring buffer holding the last [`LOG_RING_CAPACITY`] log entries.
pub type LogStore = Arc<Mutex<VecDeque<LogEntry>>>;

/// Create an empty [`LogStore`].
pub fn new_log_store() -> LogStore {
    Arc::new(Mutex::new(VecDeque::with_capacity(LOG_RING_CAPACITY)))
}

/// Append `entry` to the store, evicting the oldest entry when full.
fn push_log(store: &LogStore, entry: LogEntry) {
    if let Ok(mut ring) = store.lock() {
        if ring.len() == LOG_RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(entry);
    }
}

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
    /// Optional admin token; when set, `/admin/*` routes require `X-Admin-Token` to match.
    pub admin_token: Option<String>,
    /// Upstream provider configurations (keys resolved from env at request time).
    /// Wrapped in `RwLock` so `/admin/providers/:id/toggle` can mutate them at runtime.
    providers: Arc<RwLock<Vec<ProviderConfig>>>,
    /// Runtime-mutable configuration (rate limits, retry policy).
    pub runtime_config: Arc<RwLock<admin::RuntimeConfig>>,
    /// In-memory request metrics (shared across all clones via interior `Arc`s).
    pub metrics: MetricsStore,
    /// Per-provider token-bucket rate limiter.
    pub rate_limiter: RateLimiterStore,
    /// Optional structured JSONL audit logger; enabled when `SUBSTRATE_AUDIT_LOG` is set.
    pub audit_logger: Option<AuditLogger>,
    /// Per-session token/cost budget tracker.
    pub budget_store: BudgetStore,
    /// Budget limits from environment variables.
    pub budget_config: Arc<BudgetConfig>,
    /// In-memory ring buffer of the last 100 request audit entries.
    pub log_store: LogStore,
    /// Live-reloadable config; updated when the TOML config file changes on disk.
    pub live_config: Arc<TokioRwLock<FileConfig>>,
}

impl AppState {
    /// Wire concrete adapters under `state_dir` (creates subdirs as needed).
    pub fn new(state_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let routing: Arc<dyn RoutingPort> = Arc::new(PhenotypeRouterAdapter::new());
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
        let audit_logger = std::env::var("SUBSTRATE_AUDIT_LOG").ok().and_then(|p| {
            AuditLogger::new(std::path::Path::new(&p))
                .map_err(|e| eprintln!("audit log init failed: {e}"))
                .ok()
        });
        Ok(Self {
            routing,
            mailbox,
            config,
            auth_token: None,
            admin_token: std::env::var("SUBSTRATE_ADMIN_TOKEN").ok(),
            providers: Arc::new(RwLock::new(crate::config::builtin_providers())),
            runtime_config: Arc::new(RwLock::new(admin::RuntimeConfig::default())),
            metrics: MetricsStore::new(),
            rate_limiter: RateLimiterStore::new(),
            audit_logger,
            budget_store: BudgetStore::new(),
            budget_config: Arc::new(BudgetConfig::from_env()),
            log_store: new_log_store(),
            live_config: Arc::new(TokioRwLock::new(FileConfig::default())),
        })
    }

    /// Override the live config (e.g. pre-loaded from a TOML file at startup).
    pub fn with_live_config(mut self, cfg: FileConfig) -> Self {
        self.live_config = Arc::new(TokioRwLock::new(cfg));
        self
    }

    /// Attach an optional bearer token for protected routes.
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    /// Attach an optional admin token for `/admin/*` routes.
    pub fn with_admin_token(mut self, token: Option<String>) -> Self {
        self.admin_token = token;
        self
    }

    /// Override the provider list (e.g. from `GatewayConfig.providers`).
    /// If not called, `AppState::new` falls back to `builtin_providers()`.
    pub fn with_providers(mut self, providers: Vec<ProviderConfig>) -> Self {
        self.providers = Arc::new(RwLock::new(providers));
        self
    }

    /// Override the rate limiter (e.g. with pre-seeded buckets in tests).
    pub fn with_rate_limiter(mut self, rl: RateLimiterStore) -> Self {
        self.rate_limiter = rl;
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
        admin_token: None,
        providers: Arc::new(RwLock::new(crate::config::builtin_providers())),
        runtime_config: Arc::new(RwLock::new(admin::RuntimeConfig::default())),
        metrics: MetricsStore::new(),
        rate_limiter: RateLimiterStore::new(),
        audit_logger: None,
        budget_store: BudgetStore::new(),
        budget_config: Arc::new(BudgetConfig {
            max_tokens_per_session: None,
            max_cost_usd_per_session: None,
        }),
        log_store: new_log_store(),
        live_config: Arc::new(TokioRwLock::new(FileConfig::default())),
    })
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the axum router for `state`.
pub fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/models", get(models_handler))
        .route("/management/config", post(management_config_handler))
        .route("/a2a/messages", post(a2a_send_handler))
        .route("/a2a/inbox", get(a2a_inbox_handler))
        .route("/a2a/tasks", get(a2a_tasks_handler))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let admin_routes = Router::new()
        .route(
            "/admin/providers/{id}/toggle",
            post(admin::toggle_provider_handler),
        )
        .route("/admin/config", put(admin::update_config_handler))
        .route(
            "/admin/budget/reset/{session_id}",
            post(admin::budget_reset_handler),
        )
        .route(
            "/admin/metrics/reset",
            post(admin::admin_metrics_reset_handler),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin::require_admin_token,
        ));

    Router::new()
        .route("/healthz", get(healthz_handler))
        .route("/health", get(health_handler))
        .route("/health/providers", get(health_providers_handler))
        .route("/metrics", get(metrics_handler))
        .route("/metrics/prometheus", get(metrics_prometheus_handler))
        .route("/metrics/reset", post(metrics_reset_handler))
        .route("/budget/{session_id}", get(budget_handler))
        .route("/logs", get(logs_handler))
        .merge(protected)
        .merge(admin_routes)
        .with_state(state)
}

/// Bind and serve the gateway using `config`.
///
/// If `SUBSTRATE_CONFIG_FILE` env var points to a TOML file, a [`config_watcher::ConfigWatcher`]
/// is spawned.  Changes to that file update `AppState::live_config` in place without restart.
pub async fn serve(config: GatewayConfig) -> anyhow::Result<()> {
    // Load initial FileConfig from disk (if configured); fall back to defaults.
    let config_file_path = std::env::var("SUBSTRATE_CONFIG_FILE")
        .ok()
        .map(std::path::PathBuf::from);
    let initial_file_cfg = config_file_path
        .as_ref()
        .and_then(|p| FileConfig::from_file(p).ok())
        .unwrap_or_default();

    let state = AppState::new(&config.state_dir)?
        .with_auth_token(config.auth_token)
        .with_providers(config.providers)
        .with_live_config(initial_file_cfg.clone());

    // Spawn the hot-reload task if a config file path is set.
    if let Some(path) = config_file_path {
        let live_config = Arc::clone(&state.live_config);
        let (tx, mut rx) = watch::channel(initial_file_cfg);
        // Spawn the watcher (runs in the background via its internal thread).
        let _watcher = config_watcher::ConfigWatcher::new(path, tx)
            .map_err(|e| anyhow::anyhow!("config watcher init failed: {e}"))?;
        // Spawn a task that keeps `live_config` in sync when the channel receives.
        tokio::spawn(async move {
            // Keep _watcher alive for the duration of the task.
            let _keep = _watcher;
            while rx.changed().await.is_ok() {
                let new_cfg = rx.borrow_and_update().clone();
                *live_config.write().await = new_cfg;
            }
        });
    }

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

// ---------------------------------------------------------------------------
// Structured health response types
// ---------------------------------------------------------------------------

/// Counts of configured upstream providers.
#[derive(Debug, Serialize)]
pub struct ProviderCounts {
    /// Total number of configured providers.
    pub total: usize,
    /// Providers whose API-key env var is present and non-empty.
    pub enabled: usize,
}

/// Response body for `GET /health`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    /// Crate version from `Cargo.toml`.
    pub version: &'static str,
    /// Seconds the process has been running (`std::time::SystemTime`-based).
    pub uptime_seconds: u64,
    pub providers: ProviderCounts,
    /// RFC 3339 timestamp of when this response was generated.
    pub timestamp: String,
}

/// One entry in `GET /health/providers`.
#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    pub name: String,
    /// `true` when the API-key env var is set and non-empty.
    pub enabled: bool,
}

/// Response body for `GET /health/providers`.
#[derive(Debug, Serialize)]
pub struct ProvidersHealthResponse {
    pub providers: Vec<ProviderStatus>,
}

// ---------------------------------------------------------------------------
// Health handlers
// ---------------------------------------------------------------------------

/// `GET /health` — structured liveness + provider summary.
async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let providers_snap = state.providers.read().expect("providers lock poisoned");
    let total = providers_snap.len();
    let enabled = providers_snap
        .iter()
        .filter(|p| p.enabled && p.resolve_api_key().is_some())
        .count();

    let uptime_seconds = std::time::SystemTime::now()
        .duration_since(*PROCESS_START)
        .unwrap_or_default()
        .as_secs();

    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds,
        providers: ProviderCounts { total, enabled },
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// `GET /health/providers` — per-provider name + enabled status.
async fn health_providers_handler(State(state): State<AppState>) -> impl IntoResponse {
    let providers_snap = state.providers.read().expect("providers lock poisoned");
    let providers = providers_snap
        .iter()
        .map(|p| ProviderStatus {
            name: p.name.clone(),
            enabled: p.enabled && p.resolve_api_key().is_some(),
        })
        .collect();
    Json(ProvidersHealthResponse { providers })
}

/// Process start time captured once at module load.
static PROCESS_START: std::sync::LazyLock<std::time::SystemTime> =
    std::sync::LazyLock::new(std::time::SystemTime::now);

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

/// Unified `/v1/chat/completions` handler.
///
/// When `body.stream == true`, returns `text/event-stream` SSE chunks ending
/// with `data: [DONE]\n\n`.  Otherwise returns a single JSON completion object.
///
/// Errors are surfaced as HTTP 400/500 — never swallowed silently.
async fn chat_completions_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    // Extract or generate a session_id from the X-Session-Id header.
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Check budget before forwarding to upstream.
    if let Err(exceeded) =
        budget::check_budget(&state.budget_store, &session_id, &state.budget_config)
    {
        let body_json = serde_json::json!({ "error": exceeded.to_string() });
        let response = axum::response::Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(body_json.to_string()))
            .expect("static 429 budget response construction must succeed");
        return Ok(response);
    }

    // Resolve provider name for rate-limiting from the model field (e.g. "openai/gpt-4").
    let rl_provider = body
        .model
        .split('/')
        .next()
        .unwrap_or("unknown")
        .to_string();
    if let Err(RateLimitError {
        provider: _,
        retry_after_secs,
    }) = state.rate_limiter.check_and_consume(&rl_provider)
    {
        let retry_str = format!("{:.0}", retry_after_secs.ceil());
        let body_json = serde_json::json!({
            "error": format!("rate limit exceeded for provider '{rl_provider}'; retry after {retry_str}s")
        });
        let response = axum::response::Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .header("Retry-After", retry_str)
            .body(axum::body::Body::from(body_json.to_string()))
            .expect("static 429 response construction must succeed");
        return Ok(response);
    }

    let t0 = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let req_model = body.model.clone();
    let req_provider = req_model.split('/').next().unwrap_or("unknown").to_string();
    let providers_snap: Vec<ProviderConfig> = state
        .providers
        .read()
        .expect("providers lock poisoned")
        .iter()
        .filter(|p| p.enabled)
        .cloned()
        .collect();

    if body.stream {
        let stream = complete_chat_stream(state.routing.as_ref(), &body, &providers_snap)
            .await
            .map_err(|e| {
                let latency_ms = t0.elapsed().as_millis() as u64;
                state.metrics.record("stream", latency_ms, true);
                if let Some(logger) = &state.audit_logger {
                    let _ = logger.write(&AuditEntry {
                        timestamp_ms,
                        provider: rl_provider.clone(),
                        model: body.model.clone(),
                        request_id: request_id.clone(),
                        status: 400,
                        latency_ms,
                        input_tokens: None,
                        output_tokens: None,
                        error: Some(e.to_string()),
                    });
                }
                push_log(
                    &state.log_store,
                    LogEntry {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        provider: req_provider.clone(),
                        model: req_model.clone(),
                        status_code: 500,
                        latency_ms,
                    },
                );
                ApiError::bad_request(e)
            })?;
        let latency_ms = t0.elapsed().as_millis() as u64;
        state.metrics.record("stream", latency_ms, false);
        if let Some(logger) = &state.audit_logger {
            let _ = logger.write(&AuditEntry {
                timestamp_ms,
                provider: rl_provider,
                model: body.model.clone(),
                request_id,
                status: 200,
                latency_ms,
                input_tokens: None,
                output_tokens: None,
                error: None,
            });
        }
        push_log(
            &state.log_store,
            LogEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                provider: req_provider.clone(),
                model: req_model.clone(),
                status_code: 200,
                latency_ms,
            },
        );
        Ok(StreamingResponseBuilder::sse_stream(stream))
    } else {
        let response = complete_chat(state.routing.as_ref(), &body, &providers_snap)
            .await
            .map_err(|e| {
                let latency_ms = t0.elapsed().as_millis() as u64;
                state.metrics.record("unknown", latency_ms, true);
                if let Some(logger) = &state.audit_logger {
                    let _ = logger.write(&AuditEntry {
                        timestamp_ms,
                        provider: rl_provider.clone(),
                        model: body.model.clone(),
                        request_id: request_id.clone(),
                        status: 400,
                        latency_ms,
                        input_tokens: None,
                        output_tokens: None,
                        error: Some(e.to_string()),
                    });
                }
                push_log(
                    &state.log_store,
                    LogEntry {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        provider: req_provider.clone(),
                        model: req_model.clone(),
                        status_code: 500,
                        latency_ms,
                    },
                );
                ApiError::bad_request(e)
            })?;
        let provider = response
            .model
            .split('/')
            .next()
            .unwrap_or("unknown")
            .to_string();
        let latency_ms = t0.elapsed().as_millis() as u64;
        state.metrics.record(&provider, latency_ms, false);
        // Record budget usage. The current ChatCompletionResponse does not carry
        // upstream token counts; record 0 tokens / estimated 0 cost so the session
        // is initialised and future callers can accumulate real usage once the
        // response shape includes a `usage` field.
        let cost = budget::estimate_cost(&response.model, 0, 0);
        budget::record_usage(&state.budget_store, &session_id, 0, cost);
        if let Some(logger) = &state.audit_logger {
            let _ = logger.write(&AuditEntry {
                timestamp_ms,
                provider: provider.clone(),
                model: response.model.clone(),
                request_id,
                status: 200,
                latency_ms,
                input_tokens: None,
                output_tokens: None,
                error: None,
            });
        }
        push_log(
            &state.log_store,
            LogEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                provider,
                model: req_model,
                status_code: 200,
                latency_ms,
            },
        );
        Ok(Json(response).into_response())
    }
}

// ---------------------------------------------------------------------------
// Budget handler
// ---------------------------------------------------------------------------

/// `GET /budget/:session_id` — return current token/cost usage for a session.
async fn budget_handler(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<budget::SessionBudgetSnapshot>, ApiError> {
    budget::get_session(&state.budget_store, &session_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("session not found: {session_id}")))
}

// ---------------------------------------------------------------------------
// Metrics handlers
// ---------------------------------------------------------------------------

/// `GET /metrics` — point-in-time snapshot of request counters plus rate-limit hits.
async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut snap = state.metrics.snapshot();
    snap.rate_limit_hits = state.rate_limiter.hits_snapshot();
    Json(snap)
}

/// `GET /metrics/prometheus` — Prometheus text exposition format (version 0.0.4).
async fn metrics_prometheus_handler(State(state): State<AppState>) -> impl IntoResponse {
    let rate_limit_hits = state.rate_limiter.hits_snapshot();
    let body = state.metrics.prometheus_text(&rate_limit_hits);
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
        .body(axum::body::Body::from(body))
        .expect("static prometheus response construction must succeed")
}

/// `POST /metrics/reset` — zero all counters and return empty snapshot.
async fn metrics_reset_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics.reset();
    Json(state.metrics.snapshot())
}

/// `GET /logs` — return the last 100 audit log entries as a JSON array.
async fn logs_handler(State(state): State<AppState>) -> impl IntoResponse {
    let entries: Vec<LogEntry> = state
        .log_store
        .lock()
        .map(|ring| ring.iter().cloned().collect())
        .unwrap_or_default();
    Json(entries)
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
pub mod sla;
pub mod middleware_chain;
pub mod conn_pool;
pub mod tier;
pub mod budget_tracker;
pub mod token_bucket;
pub mod circuit_breaker2;
pub mod semaphore;
pub mod lru_cache;
pub mod shard;
pub mod ttl_map;
pub mod glob_match;
pub mod ini_parser;
pub mod utf8_count;
pub mod bytebuf;
pub mod bitfield;
pub mod result_ext;
pub mod sorted_set;
pub mod percentile;
pub mod byte_size;
pub mod url_parse;
pub mod flag_set;
pub mod hex;
pub mod bloom_gw;
pub mod ipv4;
pub mod crc32;
