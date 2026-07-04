//! HTTP client for the gateway API.
//!
//! Queries the substrate gateway for health, A2A tasks, and management config.
//! Wraps `reqwest` with auth token injection and JSON deserialisation.

use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Lightweight client that speaks to the substrate gateway.
#[derive(Clone, Debug)]
pub struct GatewayClient {
    base_url: String,
    client: reqwest::Client,
}

impl GatewayClient {
    /// Build a client from a gateway URL and optional bearer token.
    pub fn new(base_url: String, auth_token: Option<String>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(token) = auth_token {
            let value = format!("Bearer {token}");
            headers.insert(reqwest::header::AUTHORIZATION, value.parse().unwrap());
        }
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .default_headers(headers)
            .build()
            .expect("reqwest client build");
        Self { base_url, client }
    }

    // ── health ──────────────────────────────────────────────────────────

    /// GET /healthz — returns `true` when the gateway responds OK.
    pub async fn healthz(&self) -> anyhow::Result<bool> {
        let url = format!("{}/healthz", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.status().is_success())
    }

    // ── A2A tasks ───────────────────────────────────────────────────────

    /// GET /a2a/tasks?team= — list tasks tracked by the gateway.
    pub async fn list_tasks(&self, team: &str) -> anyhow::Result<Vec<A2aTaskSummary>> {
        let url = format!("{}/a2a/tasks?team={}", self.base_url, urlencode(team));
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("GET /a2a/tasks")?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GET /a2a/tasks returned {}: {}", status, text);
        }
        let tasks: Vec<A2aTaskSummary> = resp.json().await.context("parse tasks")?;
        Ok(tasks)
    }

    // ── Management config ───────────────────────────────────────────────

    /// POST /management/config — list all config entries.
    pub async fn list_config(&self) -> anyhow::Result<Vec<ConfigEntry>> {
        let url = format!("{}/management/config", self.base_url);
        let body = serde_json::json!({ "action": "list" });
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("POST /management/config (list)")?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST /management/config returned {}: {}", status, text);
        }
        #[derive(Deserialize)]
        struct ListResponse {
            entries: Vec<ConfigEntry>,
        }
        let parsed: ListResponse = resp.json().await.context("parse config list")?;
        Ok(parsed.entries)
    }
}

// ── wire types ─────────────────────────────────────────────────────────────

/// A2A task summary returned by the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct A2aTaskSummary {
    pub id: Uuid,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Config entry from the gateway.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

// ── Service status ──────────────────────────────────────────────────────

/// Operational status of a single process-compose service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    /// HTTP probe returned 200 OK.
    Running,
    /// Connection refused or probe timed out.
    Stopped,
    /// HTTP probe returned a non-200 status code.
    Unknown(u16),
}

impl std::fmt::Display for ProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "Running"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Unknown(code) => write!(f, "Unknown({code})"),
        }
    }
}

/// Status snapshot for a single process-compose service.
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    /// Service name as declared in the compose manifest.
    pub name: String,
    /// Current operational status.
    pub status: ProcessStatus,
    /// Command used to start the service (preview only).
    pub command_preview: String,
}

/// Derive the health-probe URL for a service given its name and optional port.
///
/// If a `port` is present the probe target is `http://localhost:{port}/healthz`.
/// Services without a port (e.g. socket-only daemons) cannot be HTTP-probed and
/// return `None`.
pub fn derive_health_url(port: Option<u16>) -> Option<String> {
    port.map(|p| format!("http://localhost:{p}/healthz"))
}

impl GatewayClient {
    /// Return a live status snapshot for each service in `compositions`.
    ///
    /// Each service with a configured `port` is probed via
    /// `GET http://localhost:<port>/healthz` with a 1-second timeout:
    ///
    /// - **200 OK** → [`ProcessStatus::Running`]
    /// - **connection refused / timeout** → [`ProcessStatus::Stopped`]
    /// - **any other HTTP status** → [`ProcessStatus::Unknown(code)`]
    ///
    /// Services without a port are reported as [`ProcessStatus::Stopped`] because
    /// no HTTP endpoint is available to probe.
    pub async fn get_status(
        compositions: &[crate::proccompose::Composition],
    ) -> Vec<ServiceStatus> {
        let probe_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap_or_default();

        let mut out = Vec::new();
        for c in compositions {
            for m in &c.members {
                let status = match derive_health_url(c.port) {
                    None => ProcessStatus::Stopped,
                    Some(url) => probe_url(&probe_client, &url).await,
                };
                out.push(ServiceStatus {
                    name: format!("{}/{}", c.name, m.engine),
                    status,
                    command_preview: m.model.chars().take(60).collect(),
                });
            }
        }
        out
    }
}

/// Issue a single GET probe and map the result to a [`ProcessStatus`].
async fn probe_url(client: &reqwest::Client, url: &str) -> ProcessStatus {
    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => ProcessStatus::Running,
        Ok(resp) => ProcessStatus::Unknown(resp.status().as_u16()),
        Err(_) => ProcessStatus::Stopped,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── derive_health_url ──────────────────────────────────────────────

    #[test]
    fn derive_health_url_with_port() {
        assert_eq!(
            derive_health_url(Some(8010)),
            Some("http://localhost:8010/healthz".to_string())
        );
    }

    #[test]
    fn derive_health_url_no_port() {
        assert_eq!(derive_health_url(None), None);
    }

    #[test]
    fn derive_health_url_port_zero() {
        // Port 0 is technically valid but unusual; we still produce a URL.
        assert_eq!(
            derive_health_url(Some(0)),
            Some("http://localhost:0/healthz".to_string())
        );
    }

    // ── ProcessStatus display ──────────────────────────────────────────

    #[test]
    fn status_display_running() {
        assert_eq!(ProcessStatus::Running.to_string(), "Running");
    }

    #[test]
    fn status_display_stopped() {
        assert_eq!(ProcessStatus::Stopped.to_string(), "Stopped");
    }

    #[test]
    fn status_display_unknown() {
        assert_eq!(ProcessStatus::Unknown(503).to_string(), "Unknown(503)");
    }

    // ── get_status with no compositions ───────────────────────────────

    #[tokio::test]
    async fn get_status_empty_compositions() {
        let statuses = GatewayClient::get_status(&[]).await;
        assert!(statuses.is_empty());
    }

    // ── get_status: no-port service → Stopped ─────────────────────────

    #[tokio::test]
    async fn get_status_no_port_yields_stopped() {
        use crate::proccompose::{Composition, CompositionStatus, Member};
        use std::time::Duration;
        use uuid::Uuid;

        let comp = Composition {
            name: "forge-daemon".into(),
            status: CompositionStatus::Stopped,
            members: vec![Member {
                id: Uuid::new_v4(),
                state: "stopped".into(),
                engine: "forge-daemon".into(),
                model: "cargo run -p forge_daemon".into(),
                uptime: Duration::ZERO,
                prompt_preview: String::new(),
            }],
            uptime: Duration::ZERO,
            port: None,
        };

        let statuses = GatewayClient::get_status(&[comp]).await;
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, ProcessStatus::Stopped);
    }

    // ── get_status: port with refused connection → Stopped ────────────

    #[tokio::test]
    async fn get_status_refused_connection_yields_stopped() {
        use crate::proccompose::{Composition, CompositionStatus, Member};
        use std::time::Duration;
        use uuid::Uuid;

        // Port 19999 is almost certainly not in use in test environments.
        let comp = Composition {
            name: "substrate-gateway".into(),
            status: CompositionStatus::Stopped,
            members: vec![Member {
                id: Uuid::new_v4(),
                state: "stopped".into(),
                engine: "substrate-gateway".into(),
                model: "cargo run -p gateway".into(),
                uptime: Duration::ZERO,
                prompt_preview: String::new(),
            }],
            uptime: Duration::ZERO,
            port: Some(19999),
        };

        let statuses = GatewayClient::get_status(&[comp]).await;
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, ProcessStatus::Stopped);
    }
}

/// Simple URL encoding for query params.
fn urlencode(s: &str) -> String {
    urlencoding(s)
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}
