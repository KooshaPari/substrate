//! `sharecli gateway` -- lightweight HTTP REST surface that exposes the
//! sharecli utility + cast module catalog as observable JSON endpoints.
//!
//! Endpoints (bind: `--bind`, default `127.0.0.1:8081`):
//!
//!   GET /health          -> liveness JSON with Backbone-2 family tag
//!   GET /v1/casts        -> list of registered cast subcommands
//!   GET /v1/casts/:name  -> details for a single cast subcommand
//!   GET /v1/util         -> list of bundled utility modules
//!   GET /v1/util/:name   -> details for a single utility module
//!   GET /v1/modules      -> combined catalog (casts + util)
//!   GET /v1/splash       -> ASCII splash banner (Backbone-2 family)
//!
//! Distinct from `serve` (which streams live process + thermal state over
//! WebSocket on :9000 by default). `gateway` exposes the static catalog
//! that an MCP / A2A / dashboard client would introspect before driving
//! sharecli via the CLI surface.
//!
//! wraps: axum 0.8, tokio 1 (per xDD wrap-over-handroll mandate)

use anyhow::Result;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tracing::info;

// ---------------------------------------------------------------------------
// Catalog (pure; unit-tested without I/O -- see `tests` module below)
// ---------------------------------------------------------------------------

const SPLASH: &str = r#"
   _______ _    _ ______ _____  _____ _____  ______
  / ______| || ||  ____|  __ \|_   _|  __ \|  ____|
 | (___ | || || |__  | |__) | | | | |  | | |__
  \___ \| ||__||  __| |  _  /  | | | |  | |  __|
  ____) |__   || |____| | \ \_ | |_| |__| | |____
 |_____/   |_||______|_|  \__\|______\____/|______|
"#;

const FAMILY: &str = "backbone-2";

const CAST_CATALOG: &[(&str, &str)] = &[
    ("register",   "Register a pane: `cast register <name> <address>`"),
    ("unregister", "Unregister a pane by name"),
    ("list",       "List all registered panes"),
    ("send",       "Send text to a registered pane"),
    ("where",      "Show the on-disk path of the pane-map file"),
];

const UTIL_CATALOG: &[(&str, &str)] = &[
    ("base85",   "Base85 encode / decode"),
    ("csv",      "Build a CSV row from --row entries"),
    ("crc",      "CRC64 checksum"),
    ("hash",     "xxhash3 / xxtea digest"),
    ("json",     "JSON pretty-print / validate"),
    ("md-table", "Render markdown table"),
    ("sha",      "SHA1 / SHA256 digest"),
    ("skiplist", "Walk the bundled skiplist"),
    ("trie",     "Radix-trie lookup"),
    ("url",      "URL percent-encode / decode"),
    ("uuid",     "APFS UUID helper"),
    ("xml",      "XML escape / unescape"),
];

fn http_error(code: &str, msg: &str) -> Json<Value> {
    Json(json!({ "error": code, "message": msg }))
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "sharecli-gateway",
        "family": FAMILY,
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": [
            "/health", "/v1/modules", "/v1/casts", "/v1/util", "/v1/splash"
        ],
    }))
}

async fn splash() -> impl IntoResponse {
    let body = format!(
        "{SPLASH}\nsharecli gateway ({FAMILY}) - http REST catalog server\n"
    );
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

async fn list_modules() -> Json<Value> {
    let casts: Vec<Value> = CAST_CATALOG
        .iter()
        .map(|(n, d)| json!({ "name": n, "kind": "cast", "description": d }))
        .collect();
    let utils: Vec<Value> = UTIL_CATALOG
        .iter()
        .map(|(n, d)| json!({ "name": n, "kind": "util", "description": d }))
        .collect();
    Json(json!({ "casts": casts, "util": utils, "family": FAMILY }))
}

async fn list_casts() -> Json<Value> {
    Json(json!({
        "kind": "cast",
        "count": CAST_CATALOG.len(),
        "casts": CAST_CATALOG.iter()
            .map(|(n, d)| json!({ "name": n, "description": d }))
            .collect::<Vec<_>>(),
    }))
}

async fn list_util() -> Json<Value> {
    Json(json!({
        "kind": "util",
        "count": UTIL_CATALOG.len(),
        "util": UTIL_CATALOG.iter()
            .map(|(n, d)| json!({ "name": n, "description": d }))
            .collect::<Vec<_>>(),
    }))
}

async fn cast_details(Path(name): Path<String>) -> impl IntoResponse {
    match CAST_CATALOG.iter().find(|(n, _)| *n == name) {
        Some((n, d)) => (
            StatusCode::OK,
            Json(json!({
                "kind": "cast",
                "name": n,
                "description": d,
                "pub_fns": match *n {
                    "register" => vec!["register_pane"],
                    "unregister" => vec!["unregister_pane"],
                    "list" => vec!["list_panes"],
                    "send" => vec!["send_to_pane"],
                    "where" => vec!["registry_path"],
                    _ => vec![],
                },
                "backbone": FAMILY,
            })),
        ).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "kind": "cast", "name": name })),
        ).into_response(),
    }
}

async fn util_details(Path(name): Path<String>) -> impl IntoResponse {
    match UTIL_CATALOG.iter().find(|(n, _)| *n == name) {
        Some((n, d)) => (
            StatusCode::OK,
            Json(json!({
                "kind": "util",
                "name": n,
                "description": d,
                "backbone": FAMILY,
            })),
        ).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "kind": "util", "name": name })),
        ).into_response(),
    }
}

async fn not_found(uri: axum::http::Uri) -> Json<Value> {
    http_error("not_found", &format!("no route for {}", uri.path()))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/modules", get(list_modules))
        .route("/v1/casts", get(list_casts))
        .route("/v1/casts/:name", get(cast_details))
        .route("/v1/util", get(list_util))
        .route("/v1/util/:name", get(util_details))
        .route("/v1/splash", get(splash))
        .fallback(not_found)
}

// ---------------------------------------------------------------------------
// Entry point -- called from `Commands::Gateway` in `main.rs`.
// ---------------------------------------------------------------------------

/// Run the gateway server. Blocks until Ctrl-C / SIGTERM.
///
/// `on_conflict` mirrors the existing `serve` policy (abort | attach |
/// replace) but is currently advisory only -- this surface is read-mostly
/// (GETs) so conflicts are rare. Future writes will reuse `serve_lock`.
pub async fn run(bind: SocketAddr) -> Result<()> {
    let app = router();

    let pulse = "\x1b[38;2;63;185;80m"; // Backbone-2 #3fb950
    let amber = "\x1b[38;2;210;153;34m"; // Backbone-2 #d29922
    let reset = "\x1b[0m";

    println!("{pulse}{SPLASH}{reset}");
    println!(
        "{amber}sharecli gateway{reset} :: {FAMILY} family :: http://{bind}",
    );
    info!(%bind, family = FAMILY, "sharecli gateway listening");

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("sharecli: shutdown signal received");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut sig) = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        ) {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = term => {}
    }
}

// ---------------------------------------------------------------------------
// Unit tests (pure catalog/handler-level; integration runs over the wire).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casts_have_unique_names() {
        let mut names: Vec<_> = CAST_CATALOG.iter().map(|(n, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), CAST_CATALOG.len(), "cast names must be unique");
    }

    #[test]
    fn utils_have_unique_names() {
        let mut names: Vec<_> = UTIL_CATALOG.iter().map(|(n, _)| *n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), UTIL_CATALOG.len(), "util names must be unique");
    }

    #[test]
    fn family_tag_is_backbone_2() {
        assert_eq!(FAMILY, "backbone-2");
    }

    #[test]
    fn http_error_shape() {
        let j = http_error("nope", "msg");
        // We cannot await Json directly here; instead serialize to value.
        let v = serde_json::to_value(&j.0).unwrap();
        assert_eq!(v["error"], "nope");
        assert_eq!(v["message"], "msg");
    }
}
