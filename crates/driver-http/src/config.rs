//! HTTP server configuration from environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Runtime configuration for the substrate HTTP driver.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Socket address to bind (e.g. `127.0.0.1:8080`).
    pub bind: SocketAddr,
    /// Root directory for `.substrate` state (store, mailbox, sqlite).
    pub state_dir: PathBuf,
    /// Optional bearer token; when set, all routes except `/healthz` require auth.
    pub auth_token: Option<String>,
}

impl HttpConfig {
    /// Load configuration from the process environment.
    ///
    /// | Variable | Default |
    /// |----------|---------|
    /// | `SUBSTRATE_HTTP_BIND` | `127.0.0.1:8080` |
    /// | `SUBSTRATE_STATE_DIR` | `./.substrate` |
    /// | `SUBSTRATE_HTTP_AUTH_TOKEN` | unset (no auth) |
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let bind = std::env::var("SUBSTRATE_HTTP_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".into())
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid SUBSTRATE_HTTP_BIND: {e}"))?;
        let state_dir = std::env::var("SUBSTRATE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".substrate"));
        let auth_token = std::env::var("SUBSTRATE_HTTP_AUTH_TOKEN").ok();
        Ok(HttpConfig {
            bind,
            state_dir,
            auth_token,
        })
    }
}
