//! HTTP route handlers for `gateway-tools serve`.
//!
//! Wraps `axum` 0.8 (per xDD wrap-over-handroll mandate). The HTTP surface
//! intentionally mirrors `inspect_registry()` — the registry already curated
//! for the CLI is reused for the REST route, so there is one source of truth
//! for module name, description, and public-fn list.
//!
//! Routes (all return JSON unless noted):
//!   GET /health                     -> `{ "status": "ok", "service": "..." }`
//!   GET /v1/modules                 -> list of `{ name, fns }`
//!   GET /v1/modules/:name           -> `{ name, description, public_fns }`
//!   GET /v1/splash                  -> ASCII splash as text/plain
//!   *                                -> 404 `{ "error": "not_found", "path": "..." }`

use std::sync::Arc;

use axum::response::IntoResponse;

/// One row in the static module registry. The CLI's `inspect_registry()` is
/// the human-readable sibling; this struct is the same data in a typed form so
/// axum handlers can return JSON without re-parsing strings.
///
/// Tuple shape: `(name, description, public_fn_names, fn_count)`.
pub type ModuleEntry = (&'static str, &'static str, &'static [&'static str], usize);

/// Modules known to exist in the `gateway` crate on `main` (post-L108).
/// Counts are the size of the in-binary `inspect_registry()` row, kept in sync
/// for `/v1/modules/:name` parity. Hard-coded per task spec — no introspection.
pub const KNOWN_MODULES: &[ModuleEntry] = &[
    (
        "oidc_jwt",
        "OpenID Connect JWT decoder — header + payload + custom claim map.",
        &[
            "decode",
            "validate",
            "header",
            "payload",
            "claims",
            "signing_alg",
            "issuer",
            "audience",
        ],
        8,
    ),
    (
        "prometheus_scrape",
        "Pull-style Prometheus scrape endpoint builder.",
        &[
            "scrape",
            "render",
            "metric_families",
            "registry",
            "counter",
            "gauge",
            "histogram",
            "summary",
            "labels",
            "exposition",
            "content_type",
            "filters",
        ],
        12,
    ),
    (
        "prometheus_exposition",
        "Prometheus text exposition format (counter/gauge/histogram/summary).",
        &[
            "render",
            "Metric",
            "MetricType",
            "histogram_buckets",
            "format_labels",
            "escape_label",
        ],
        6,
    ),
    (
        "vmstat_parser",
        "Parse /proc/vmstat key/value lines into typed records.",
        &["parse", "VmstatRecord", "key", "value", "iter"],
        5,
    ),
    (
        "http1_request",
        "Minimal HTTP/1.1 request line + header parser.",
        &["parse_request_line", "parse_headers", "Method", "Uri", "Version"],
        5,
    ),
    (
        "jwt_jwks",
        "JWKS fetcher + cache — turns a `kid` into a verification key.",
        &[
            "fetch_jwks",
            "key_for_kid",
            "JwksCache",
            "refresh",
            "verify_with_jwks",
        ],
        5,
    ),
    (
        "circuit_breaker",
        "Three-state breaker (closed/half-open/open) with explicit reset.",
        &[
            "CircuitBreaker",
            "State",
            "call",
            "on_success",
            "on_failure",
            "reset",
            "open_until",
        ],
        7,
    ),
    (
        "bounded_body",
        "Bounded body reader — caps inbound bytes to a max limit.",
        &["BoundedBody", "read", "remaining", "limit", "into_bytes"],
        5,
    ),
    (
        "streaming",
        "SSE/streaming helpers for the gateway response pipeline.",
        &[
            "SseEvent",
            "data_frame",
            "event_frame",
            "retry_frame",
            "frame_to_bytes",
            "encode",
        ],
        6,
    ),
];

/// Cheap-allocator version of `KNOWN_MODULES` for the JSON list route.
#[derive(Clone)]
pub struct ModuleSummary {
    pub name: &'static str,
    pub fns: usize,
}

/// Wrapper so axum handlers can own an `Arc<Vec<ModuleSummary>>` cheaply.
#[derive(Clone)]
pub struct Registry(pub Arc<Vec<ModuleSummary>>);

impl Registry {
    pub fn new() -> Self {
        let summaries = KNOWN_MODULES
            .iter()
            .map(|(name, _desc, _fns, fn_count)| ModuleSummary { name, fns: *fn_count })
            .collect();
        Self(Arc::new(summaries))
    }

    pub fn summaries(&self) -> &[ModuleSummary] {
        &self.0
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot the static registry into an owned handle. The CLI dispatcher calls
/// this once at startup and hands the cloneable handle to `build_router`.
pub fn static_registry() -> Registry {
    Registry::new()
}

/// Build the axum `Router` wired to all the public routes.
pub fn build_router(reg: Registry) -> axum::Router {
    use axum::routing::get;

    axum::Router::new()
        .route("/health", get(health))
        .route("/v1/modules", get(list_modules))
        .route("/v1/modules/:name", get(get_module))
        .route("/v1/splash", get(splash))
        .fallback(not_found)
        .with_state(reg)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "substrate-gateway-tools",
        "color": "sync-violet:#a371f7",
    }))
}

async fn list_modules(
    axum::extract::State(reg): axum::extract::State<Registry>,
) -> axum::Json<serde_json::Value> {
    let modules: Vec<serde_json::Value> = reg
        .summaries()
        .iter()
        .map(|m| serde_json::json!({ "name": m.name, "fns": m.fns }))
        .collect();
    axum::Json(serde_json::json!({ "modules": modules }))
}

async fn get_module(
    axum::extract::State(reg): axum::extract::State<Registry>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> axum::response::Response {
    if let Some((entry_name, desc, fns, _count)) =
        KNOWN_MODULES.iter().find(|(n, _, _, _)| *n == name.as_str())
    {
        let body = serde_json::json!({
            "name": entry_name,
            "description": desc,
            "public_fns": fns,
        });
        return axum::Json(body).into_response();
    }
    // 404 fallback — JSON body, mirrors the global fallback shape.
    not_found_inner_body(&format!("/v1/modules/{name}")).into_response()
}

async fn splash() -> axum::response::Response {
    (
        axum::http::StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        SPLASH_ART,
    )
        .into_response()
}

async fn not_found() -> axum::response::Response {
    not_found_inner_body("/").into_response()
}

/// Sync inner for the 404 body — used directly by `get_module` so we don't
/// need to spawn or await just to build a JSON body. The `async fn not_found`
/// handler above wraps this for the global fallback.
fn not_found_inner_body(path: &str) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "error": "not_found",
        "path": path,
    }))
}

// ---------------------------------------------------------------------------
// Splash — duplicated from main.rs::substrate_splash() for the text/plain route.
// Kept local so routes remain self-contained; any future single-source consolidation
// would lift both into a shared `splash::art()` const.
// ---------------------------------------------------------------------------

const SPLASH_ART: &str = r#"
   ____  _   _ ____ _____ _____ _____ ____  _____
  / ___|| | | / ___|_   _|  ___|_   _|  _ \|  __ \
  \___ \| |_| \___ \ | | | |_    | | | |_) | |  | |
   ___) |  _  |___) || | |  _|   | | |  _ <| |  | |
  |____/|_| |_|____/ |_| |_|     |_| |_| \_\_|  |_|
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    async fn body_to_json(body: axum::body::Body) -> serde_json::Value {
        let bytes = to_bytes(body, 1024 * 1024).await.expect("read body");
        serde_json::from_slice(&bytes).expect("parse json")
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_to_json(resp.into_body()).await;
        assert_eq!(json["status"], "ok");
        assert_eq!(json["service"], "substrate-gateway-tools");
    }

    #[tokio::test]
    async fn modules_list_includes_oidc_jwt() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/v1/modules")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_to_json(resp.into_body()).await;
        let names: Vec<&str> = json["modules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"oidc_jwt"));
        assert!(names.contains(&"prometheus_scrape"));
    }

    #[tokio::test]
    async fn module_detail_by_name() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/v1/modules/circuit_breaker")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_to_json(resp.into_body()).await;
        assert_eq!(json["name"], "circuit_breaker");
        assert!(json["public_fns"].as_array().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn unknown_module_returns_404() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/v1/modules/does-not-exist")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let json = body_to_json(resp.into_body()).await;
        assert_eq!(json["error"], "not_found");
    }

    #[tokio::test]
    async fn splash_route_returns_text() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/v1/splash")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("text/plain"), "got content-type `{ct}`");
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("____"));
    }
}
