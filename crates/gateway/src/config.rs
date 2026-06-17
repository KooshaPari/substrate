//! HTTP server configuration from environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Runtime configuration for the substrate gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Socket address to bind (e.g. `127.0.0.1:20128`).
    pub bind: SocketAddr,
    /// Root directory for `.substrate` state (sqlite stores).
    pub state_dir: PathBuf,
    /// Optional bearer token; when set, protected routes require auth.
    pub auth_token: Option<String>,
}

impl GatewayConfig {
    /// Load configuration from the process environment.
    ///
    /// | Variable | Default |
    /// |----------|---------|
    /// | `SUBSTRATE_GATEWAY_BIND` | `127.0.0.1:20128` |
    /// | `SUBSTRATE_STATE_DIR` | `./.substrate` |
    /// | `SUBSTRATE_GATEWAY_AUTH_TOKEN` | unset (no auth) |
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        let bind = std::env::var("SUBSTRATE_GATEWAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:20128".into())
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid SUBSTRATE_GATEWAY_BIND: {e}"))?;
        let state_dir = std::env::var("SUBSTRATE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".substrate"));
        let auth_token = std::env::var("SUBSTRATE_GATEWAY_AUTH_TOKEN").ok();
        Ok(GatewayConfig {
            bind,
            state_dir,
            auth_token,
        })
    }
}
