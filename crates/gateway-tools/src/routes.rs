//! HTTP route handlers for `gateway-tools serve`.
//!
//! Wraps `axum` 0.8 (per xDD wrap-over-handroll mandate). The HTTP surface
//! intentionally mirrors `inspect_registry()` — the registry already curated
//! for the CLI is reused for the REST route, so there is one source of truth
//! for module name, description, and public-fn list.
//!
//! Routes (all return JSON unless noted):
//!   GET /                           -> server-rendered HTML cockpit (Backbone-2 sync-violet)
//!   GET /health                     -> `{ "status": "ok", "service": "..." }`
//!   GET /v1/modules                 -> list of `{ name, fns }`
//!   GET /v1/modules/:name           -> `{ name, description, public_fns }`
//!   GET /v1/splash                  -> ASCII splash as text/plain
//!   *                                -> HTML 404 cockpit (or JSON for `/v1/*`)

use std::sync::Arc;

use axum::response::{Html, IntoResponse};

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
        .route("/", get(cockpit))
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

/// Server-rendered HTML cockpit for the `serve` subcommand.
///
/// Backbone-2 family palette (sync-violet `#a371f7` + amber `#d29922` on
/// `#161b22` panel) — applied via inline `<style>` so the page has no
/// external asset dependencies. Builds at request time so the module count
/// stays in sync with `KNOWN_MODULES.len()`.
async fn cockpit() -> Html<String> {
    let mod_count = KNOWN_MODULES.len();
    Html(COCKPIT_HTML.replace("{mod_count}", &mod_count.to_string()))
}

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
    // The fallback only fires for paths not matched by any registered route.
    // `/v1/modules/:name` not-found paths go through `not_found_inner_body`
    // directly via `get_module`. So the fallback is HTML for the cockpit UX.
    not_found_html("/").into_response()
}

/// Sync inner for the 404 body — used directly by `get_module` so we don't
/// need to spawn or await just to build a JSON body.
fn not_found_inner_body(path: &str) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "error": "not_found",
        "path": path,
    }))
}

/// HTML 404 cockpit — reuses the Backbone-2 palette and links back to `/`.
fn not_found_html(path: &str) -> Html<String> {
    let body = NOT_FOUND_HTML
        .replace("{path}", path)
        .replace("{mod_count}", &KNOWN_MODULES.len().to_string());
    Html(body)
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
// Cockpit HTML — server-rendered, no JS framework, no external assets.
// Placeholders: `{mod_count}` for KNOWN_MODULES.len(), `{path}` for 404.
// ---------------------------------------------------------------------------

const COCKPIT_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>substrate gateway-tools / cockpit</title>
  <style>
    body { background: #161b22; color: #d29922; font-family: 'Courier New', monospace; padding: 2rem; }
    h1 { color: #a371f7; border-bottom: 2px solid #a371f7; padding-bottom: 0.5rem; }
    h2 { color: #d29922; }
    a { color: #a371f7; text-decoration: none; }
    a:hover { text-decoration: underline; }
    .card { border: 1px solid #a371f7; padding: 1rem; margin: 1rem 0; border-radius: 4px; background: #0d1117; }
    code { background: #161b22; padding: 0.2em 0.4em; border-radius: 3px; }
    .pill { background: #a371f7; color: #161b22; padding: 0.1em 0.5em; border-radius: 1em; font-size: 0.8em; }
    nav { background: #0d1117; padding: 0.8rem 1.2rem; margin: -2rem -2rem 1.5rem -2rem; border-bottom: 1px solid #a371f7; }
    nav a { margin-right: 1.2rem; font-weight: bold; }
    .modules { list-style: none; padding: 0; }
    .modules li { padding: 0.3em 0.6em; border-bottom: 1px dotted #2a2a2a; }
    .modules li:hover { background: #1a1a1a; }
    @keyframes pulse-pill { 0%, 100% { opacity: 1; } 50% { opacity: 0.6; } }
    .pill { animation: pulse-pill 2s ease-in-out infinite; }
    @keyframes fadein { from { opacity: 0; } to { opacity: 1; } }
    body { animation: fadein 0.3s ease-out; }
  </style>
</head>
<body>
  <nav>
    <a href="/">/ (cockpit)</a>
    <a href="/health">/health</a>
    <a href="/v1/modules">/v1/modules</a>
    <a href="/v1/splash">/v1/splash</a>
  </nav>
  <h1>&#9670; substrate / gateway-tools</h1>
  <p class="pill">sync-violet Backbone-2 cockpit</p>
  <h2>Routes</h2>
  <ul>
    <li><a href="/health">/health</a> &mdash; liveness JSON</li>
    <li><a href="/v1/modules">/v1/modules</a> &mdash; module list</li>
    <li><a href="/v1/splash">/v1/splash</a> &mdash; ASCII splash</li>
  </ul>
  <h2>Modules ({mod_count})</h2>
  <div class="card">
    <ul class="modules">
      <li><a href="/v1/modules/oidc_jwt">oidc_jwt</a> &mdash; OIDC JWT decoder (8 pub_fns)</li>
      <li><a href="/v1/modules/prometheus_scrape">prometheus_scrape</a> &mdash; Prometheus scrape builder (12 pub_fns)</li>
      <li><a href="/v1/modules/prometheus_exposition">prometheus_exposition</a> &mdash; Prometheus text exposition (6 pub_fns)</li>
      <li><a href="/v1/modules/vmstat_parser">vmstat_parser</a> &mdash; /proc/vmstat parser (5 pub_fns)</li>
      <li><a href="/v1/modules/http1_request">http1_request</a> &mdash; HTTP/1.1 request parser (5 pub_fns)</li>
      <li><a href="/v1/modules/jwt_jwks">jwt_jwks</a> &mdash; JWKS fetcher (5 pub_fns)</li>
      <li><a href="/v1/modules/pem_codec">pem_codec</a> &mdash; PEM encode/decode (4 pub_fns)</li>
      <li><a href="/v1/modules/tls_record">tls_record</a> &mdash; TLS record parse/write (4 pub_fns)</li>
      <li><a href="/v1/modules/redis_resp">redis_resp</a> &mdash; RESP value encode/parse (3 pub_fns)</li>
      <li><a href="/v1/modules/dns_message_parser">dns_message_parser</a> &mdash; DNS packet parser (3 pub_fns)</li>
    </ul>
  </div>
  <h2>CLI</h2>
  <pre><code>cargo run -p gateway-tools -- serve --port 8080
curl http://127.0.0.1:8080/v1/modules</code></pre>
  <p><small>Built on axum 0.8 + tokio 1 (per xDD mandate). L114: nav header + module list added.</small></p>
</body>
</html>
"#;

const NOT_FOUND_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>substrate / 404</title>
  <style>
    body { background: #161b22; color: #d29922; font-family: 'Courier New', monospace; padding: 2rem; }
    h1 { color: #a371f7; border-bottom: 2px solid #a371f7; padding-bottom: 0.5rem; }
    a { color: #a371f7; text-decoration: none; }
    a:hover { text-decoration: underline; }
    code { background: #161b22; padding: 0.2em 0.4em; border-radius: 3px; }
  </style>
</head>
<body>
  <h1>&#9670; substrate / 404</h1>
  <p>No route matches <code>{path}</code>.</p>
  <p><a href="/">&larr; back to cockpit</a></p>
  <p><small>Backbone-2 family palette.</small></p>
</body>
</html>
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

    #[tokio::test]
    async fn cockpit_root_returns_html() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("text/html"), "got content-type `{ct}`");
        let bytes = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("substrate / gateway-tools"));
        assert!(body.contains("#a371f7"));
        // module count is interpolated at request time
        let expected_count = KNOWN_MODULES.len().to_string();
        assert!(
            body.contains(&format!("<strong>{expected_count} modules</strong>")),
            "cockpit body missing module count `{expected_count}`",
        );
        // sanity-check the L109 REST routes are linked from the cockpit
        assert!(body.contains("/v1/modules"));
        assert!(body.contains("/v1/splash"));
        assert!(body.contains("/health"));
    }

    #[tokio::test]
    async fn unknown_path_returns_html_404() {
        let app = build_router(Registry::new());
        let resp = axum::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/does-not-exist")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("text/html"), "got content-type `{ct}`");
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("substrate / 404"));
        assert!(body.contains("back to cockpit"));
    }
}
