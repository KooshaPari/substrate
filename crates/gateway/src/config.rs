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
    /// Base URL for the upstream OpenAI-compatible provider.
    pub upstream_url: String,
    /// API key for the upstream OpenAI-compatible provider.
    pub upstream_key: String,
}

impl GatewayConfig {
    /// Load configuration from the process environment.
    ///
    /// | Variable | Default |
    /// |----------|---------|
    /// | `SUBSTRATE_GATEWAY_BIND` | `127.0.0.1:20128` |
    /// | `SUBSTRATE_STATE_DIR` | `./.substrate` |
    /// | `SUBSTRATE_GATEWAY_AUTH_TOKEN` | unset (no auth) |
    /// | `GATEWAY_UPSTREAM_URL` | required |
    /// | `GATEWAY_UPSTREAM_KEY` | required |
    pub fn from_env() -> anyhow::Result<Self> {
        let _ = dotenvy::dotenv();
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
        {
            let _ = dotenvy::from_path(home.join(".env"));
        }

        let bind = std::env::var("SUBSTRATE_GATEWAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:20128".into())
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid SUBSTRATE_GATEWAY_BIND: {e}"))?;
        let state_dir = std::env::var("SUBSTRATE_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".substrate"));
        let auth_token = std::env::var("SUBSTRATE_GATEWAY_AUTH_TOKEN").ok();
        let upstream_url = std::env::var("GATEWAY_UPSTREAM_URL")
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .map_err(|_| anyhow::anyhow!("GATEWAY_UPSTREAM_URL must be set"))?;
        if upstream_url.is_empty() {
            return Err(anyhow::anyhow!("GATEWAY_UPSTREAM_URL must not be empty"));
        }
        let upstream_key = std::env::var("GATEWAY_UPSTREAM_KEY")
            .map(|value| value.trim().to_string())
            .map_err(|_| anyhow::anyhow!("GATEWAY_UPSTREAM_KEY must be set"))?;
        if upstream_key.is_empty() {
            return Err(anyhow::anyhow!("GATEWAY_UPSTREAM_KEY must not be empty"));
        }

        Ok(GatewayConfig {
            bind,
            state_dir,
            auth_token,
            upstream_url,
            upstream_key,
        })
    }
}
