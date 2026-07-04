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
