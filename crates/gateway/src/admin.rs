//! Admin REST API for `substrate-gateway`.
//!
//! All routes under `/admin/*` require the `X-Admin-Token` header to match the
//! value of the `SUBSTRATE_ADMIN_TOKEN` environment variable.  Missing or
//! wrong tokens produce `401 Unauthorized`.
//!
//! # Endpoints
//! - `POST /admin/providers/:id/toggle` — enable/disable a provider at runtime
//! - `PUT  /admin/config`               — update runtime rate-limit / retry config
//! - `POST /admin/budget/reset/:session_id` — clear a session's budget
//! - `POST /admin/metrics/reset`        — zero all request counters

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

// ---------------------------------------------------------------------------
// Runtime config (mutable at runtime via PUT /admin/config)
// ---------------------------------------------------------------------------

/// Rate-limit settings that can be updated at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per second per provider (must be positive).
    pub rps: f64,
    /// Burst allowance per provider (must be >= 1).
    pub burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            rps: 10.0,
            burst: 20,
        }
    }
}

/// Retry policy settings that can be updated at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicyConfig {
    /// Maximum number of attempts (including the first).
    pub max_attempts: u32,
    /// Base exponential back-off delay in milliseconds.
    pub base_delay_ms: u64,
    /// Hard cap on computed delay in milliseconds.
    pub max_delay_ms: u64,
}

impl Default for RetryPolicyConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 100,
            max_delay_ms: 5_000,
        }
    }
}

/// Top-level runtime configuration managed by the admin API.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeConfig {
    pub rate_limit: RateLimitConfig,
    pub retry_policy: RetryPolicyConfig,
}

// ---------------------------------------------------------------------------
// Admin auth middleware
// ---------------------------------------------------------------------------

/// Axum middleware that requires `X-Admin-Token` to match `state.admin_token`.
///
/// If `state.admin_token` is `None` (env var not set) every request is rejected
/// with `401` to fail closed rather than open.
pub async fn require_admin_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = match &state.admin_token {
        Some(t) if !t.is_empty() => t.clone(),
        // No token configured → deny all admin access (fail closed).
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    let provided = req
        .headers()
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ToggleResponse {
    pub provider: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct BudgetResetResponse {
    pub session_id: String,
    /// `true` when an entry existed and was removed; `false` if it was never seen.
    pub was_present: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /admin/providers/:id/toggle`
///
/// Flips `ProviderConfig::enabled` for the provider identified by `:id`.
/// Returns `404` if no provider with that name is registered.
pub async fn toggle_provider_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ToggleResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut providers = state
        .providers
        .write()
        .map_err(|_| err_json(StatusCode::INTERNAL_SERVER_ERROR, "providers lock poisoned"))?;
    let provider = providers
        .iter_mut()
        .find(|p| p.name == id)
        .ok_or_else(|| err_json(StatusCode::NOT_FOUND, format!("provider not found: {id}")))?;
    provider.enabled = !provider.enabled;
    Ok(Json(ToggleResponse {
        provider: provider.name.clone(),
        enabled: provider.enabled,
    }))
}

/// `PUT /admin/config`
///
/// Replaces the runtime configuration atomically.  The body is validated via
/// serde; partial updates are not supported (send the full config object).
pub async fn update_config_handler(
    State(state): State<AppState>,
    Json(new_config): Json<RuntimeConfig>,
) -> Result<Json<RuntimeConfig>, (StatusCode, Json<serde_json::Value>)> {
    // Validate rate-limit fields.
    if new_config.rate_limit.rps <= 0.0 {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "rate_limit.rps must be positive",
        ));
    }
    if new_config.rate_limit.burst < 1 {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "rate_limit.burst must be >= 1",
        ));
    }
    // Validate retry fields.
    if new_config.retry_policy.max_attempts < 1 {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "retry_policy.max_attempts must be >= 1",
        ));
    }
    let mut cfg = state
        .runtime_config
        .write()
        .map_err(|_| err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock poisoned"))?;
    *cfg = new_config.clone();
    Ok(Json(new_config))
}

/// `POST /admin/budget/reset/:session_id`
///
/// Clears all token/cost budget state for `session_id`.  Idempotent: returns
/// `was_present: false` if the session was never seen.
pub async fn budget_reset_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Json<BudgetResetResponse> {
    let was_present = crate::budget::reset_session(&state.budget_store, &session_id);
    Json(BudgetResetResponse {
        session_id,
        was_present,
    })
}

/// `POST /admin/metrics/reset`
///
/// Zeroes all request counters and returns an empty snapshot.
pub async fn admin_metrics_reset_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics.reset();
    Json(state.metrics.snapshot())
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn err_json(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg.into() })))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{budget, config_watcher::FileConfig, metrics::MetricsStore, AppState};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use std::sync::{Arc, RwLock};
    use tokio::sync::RwLock as TokioRwLock;
    use tower::ServiceExt; // for `oneshot`

    // Build a minimal AppState for testing without touching the filesystem.
    fn make_state(admin_token: Option<&str>) -> AppState {
        let providers = crate::config::builtin_providers();
        AppState {
            routing: Arc::new(routing_phenotype_router::PhenotypeRouterAdapter::new()),
            mailbox: Arc::new(store_sqlite::SqliteMailboxStore::open_in_memory().unwrap()),
            config: Arc::new(store_sqlite::SqliteConfigStore::open_in_memory().unwrap()),
            auth_token: None,
            admin_token: admin_token.map(str::to_owned),
            providers: Arc::new(RwLock::new(providers)),
            runtime_config: Arc::new(RwLock::new(RuntimeConfig::default())),
            metrics: MetricsStore::new(),
            rate_limiter: crate::RateLimiterStore::new(),
            audit_logger: None,
            budget_store: crate::budget::BudgetStore::new(),
            budget_config: Arc::new(crate::budget::BudgetConfig {
                max_tokens_per_session: None,
                max_cost_usd_per_session: None,
            }),
            log_store: crate::new_log_store(),
            live_config: Arc::new(TokioRwLock::new(FileConfig::default())),
            response_cache: Arc::new(std::sync::Mutex::new(
                crate::TtlCache2::new(std::time::Duration::from_secs(300)),
            )),
        }
    }

    fn app_with_admin(state: AppState) -> Router {
        use axum::{
            middleware,
            routing::{post, put},
        };
        Router::new()
            .route(
                "/admin/providers/{id}/toggle",
                post(toggle_provider_handler),
            )
            .route("/admin/config", put(update_config_handler))
            .route(
                "/admin/budget/reset/{session_id}",
                post(budget_reset_handler),
            )
            .route("/admin/metrics/reset", post(admin_metrics_reset_handler))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                require_admin_token,
            ))
            .with_state(state)
    }

    // 1. Admin request without X-Admin-Token header is rejected with 401.
    #[tokio::test]
    async fn admin_rejects_missing_token() {
        let app = app_with_admin(make_state(Some("secret")));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/metrics/reset")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 2. Admin request with wrong token is rejected with 401.
    #[tokio::test]
    async fn admin_rejects_wrong_token() {
        let app = app_with_admin(make_state(Some("secret")));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/metrics/reset")
                    .header("x-admin-token", "wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 3. Toggle changes provider enabled state.
    #[tokio::test]
    async fn toggle_changes_provider_state() {
        let state = make_state(Some("tok"));
        let app = app_with_admin(state.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/providers/deepseek/toggle")
                    .header("x-admin-token", "tok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["provider"], "deepseek");
        // was true → now false
        assert_eq!(json["enabled"], false);
        // The state change should be visible in the Arc<RwLock<...>>
        let providers = state.providers.read().unwrap();
        let deepseek = providers.iter().find(|p| p.name == "deepseek").unwrap();
        assert!(!deepseek.enabled);
    }

    // 4. Config update is applied and returned.
    #[tokio::test]
    async fn config_update_applied() {
        let state = make_state(Some("tok"));
        let app = app_with_admin(state.clone());
        let new_cfg = serde_json::json!({
            "rate_limit": { "rps": 5.0, "burst": 10 },
            "retry_policy": { "max_attempts": 2, "base_delay_ms": 50, "max_delay_ms": 2000 }
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/config")
                    .header("x-admin-token", "tok")
                    .header("content-type", "application/json")
                    .body(Body::from(new_cfg.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cfg = state.runtime_config.read().unwrap();
        assert!((cfg.rate_limit.rps - 5.0).abs() < 1e-9);
        assert_eq!(cfg.rate_limit.burst, 10);
        assert_eq!(cfg.retry_policy.max_attempts, 2);
    }

    // 5. Budget reset clears a session.
    #[tokio::test]
    async fn budget_reset_clears_session() {
        let state = make_state(Some("tok"));
        // Pre-populate the session.
        budget::record_usage(&state.budget_store, "sess-abc", 100, 0.5);
        assert!(budget::get_session(&state.budget_store, "sess-abc").is_some());

        let app = app_with_admin(state.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/budget/reset/sess-abc")
                    .header("x-admin-token", "tok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["was_present"], true);
        // Session gone from store.
        assert!(budget::get_session(&state.budget_store, "sess-abc").is_none());
    }

    // 6. Metrics reset zeroes counters.
    #[tokio::test]
    async fn metrics_reset_zeroes_counters() {
        let state = make_state(Some("tok"));
        // Inject some counts.
        state.metrics.record("openai", 50, false);
        state.metrics.record("openai", 60, true);
        {
            let snap = state.metrics.snapshot();
            assert!(snap.total_requests > 0);
        }
        let app = app_with_admin(state.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/metrics/reset")
                    .header("x-admin-token", "tok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let snap = state.metrics.snapshot();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.total_errors, 0);
    }

    // 7. Toggle on unknown provider returns 404.
    #[tokio::test]
    async fn toggle_unknown_provider_returns_404() {
        let app = app_with_admin(make_state(Some("tok")));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/providers/no-such-provider/toggle")
                    .header("x-admin-token", "tok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // 8. Config update rejects invalid rps=0.
    #[tokio::test]
    async fn config_update_rejects_invalid_rps() {
        let app = app_with_admin(make_state(Some("tok")));
        let bad = serde_json::json!({
            "rate_limit": { "rps": 0.0, "burst": 5 },
            "retry_policy": { "max_attempts": 1, "base_delay_ms": 100, "max_delay_ms": 1000 }
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/config")
                    .header("x-admin-token", "tok")
                    .header("content-type", "application/json")
                    .body(Body::from(bad.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
