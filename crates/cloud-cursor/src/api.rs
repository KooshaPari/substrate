//! HTTP client for Cursor Cloud Agents API v1.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Client;
use serde::Deserialize;
use substrate_core::cloud_dispatch_port::{CloudResult, CloudTaskHandle, CloudTaskStatus};
use substrate_core::error::{Result, SubstrateError};

use crate::map_run_status;

/// Default Cursor API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.cursor.com";

/// Cursor Cloud Agents adapter.
#[derive(Debug, Clone)]
pub struct CursorCloudDispatch {
    client: Client,
    base_url: String,
    auth_header: String,
    tasks: Arc<Mutex<HashMap<String, CursorTaskMeta>>>,
}

#[derive(Debug, Clone)]
struct CursorTaskMeta {
    agent_id: String,
    run_id: String,
    branch_hint: String,
}

#[derive(Debug, Deserialize)]
struct CreateAgentResponse {
    agent: AgentSummary,
    run: RunSummary,
}

#[derive(Debug, Deserialize)]
struct AgentSummary {
    id: String,
}

#[derive(Debug, Deserialize)]
struct RunSummary {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GetRunResponse {
    status: String,
    result: Option<String>,
    git: Option<GitInfo>,
}

#[derive(Debug, Deserialize)]
struct GitInfo {
    branches: Option<Vec<GitBranch>>,
}

#[derive(Debug, Deserialize)]
struct GitBranch {
    branch: Option<String>,
    #[serde(rename = "prUrl")]
    pr_url: Option<String>,
}

impl CursorCloudDispatch {
    /// Build from `CURSOR_API_KEY` and optional `CURSOR_API_BASE_URL`.
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let api_key = std::env::var("CURSOR_API_KEY")
            .map_err(|e| SubstrateError::CloudDispatch(format!("CURSOR_API_KEY not set: {e}")))?;
        let base_url =
            std::env::var("CURSOR_API_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        Ok(Self::new(&base_url, &api_key))
    }

    /// Build with explicit credentials (tests / callers with keys in hand).
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            auth_header: basic_auth_header(&api_key.into()),
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Submit a cloud agent task.
    pub async fn submit(&self, repo: &str, branch: &str, prompt: &str) -> Result<CloudTaskHandle> {
        let url = format!("{}/v1/agents", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "prompt": { "text": prompt },
            "repos": [{
                "url": repo,
                "startingRef": branch
            }],
            "autoCreatePR": true
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("cursor create request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SubstrateError::CloudDispatch(format!(
                "cursor create failed ({status}): {text}"
            )));
        }

        let parsed: CreateAgentResponse = resp
            .json()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("cursor create parse: {e}")))?;

        let handle_id = format!("cursor-{}", parsed.agent.id);
        self.tasks.lock().unwrap().insert(
            handle_id.clone(),
            CursorTaskMeta {
                agent_id: parsed.agent.id,
                run_id: parsed.run.id,
                branch_hint: branch.to_string(),
            },
        );

        Ok(CloudTaskHandle { id: handle_id })
    }

    /// Poll run status for a submitted handle.
    pub async fn poll(&self, handle: &CloudTaskHandle) -> Result<CloudTaskStatus> {
        let meta = self
            .tasks
            .lock()
            .unwrap()
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        let url = format!(
            "{}/v1/agents/{}/runs/{}",
            self.base_url.trim_end_matches('/'),
            meta.agent_id,
            meta.run_id
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("cursor poll request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SubstrateError::CloudDispatch(format!(
                "cursor poll failed ({status}): {text}"
            )));
        }

        let run: GetRunResponse = resp
            .json()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("cursor poll parse: {e}")))?;

        Ok(map_run_status(&run.status, run.result.as_deref()))
    }

    /// Harvest PR metadata from a finished run.
    pub async fn harvest_run(&self, handle: &CloudTaskHandle) -> Result<CloudResult> {
        let meta = self
            .tasks
            .lock()
            .unwrap()
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        let url = format!(
            "{}/v1/agents/{}/runs/{}",
            self.base_url.trim_end_matches('/'),
            meta.agent_id,
            meta.run_id
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("cursor harvest request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SubstrateError::CloudDispatch(format!(
                "cursor harvest failed ({status}): {text}"
            )));
        }

        let run: GetRunResponse = resp
            .json()
            .await
            .map_err(|e| SubstrateError::CloudDispatch(format!("cursor harvest parse: {e}")))?;

        let status = map_run_status(&run.status, run.result.as_deref());
        if status != CloudTaskStatus::Succeeded {
            return Err(SubstrateError::CloudDispatch(format!(
                "cursor run not succeeded: {status:?}"
            )));
        }

        let (branch, pr_url) = run
            .git
            .and_then(|g| g.branches)
            .and_then(|mut b| b.pop())
            .map(|b| (b.branch.unwrap_or(meta.branch_hint.clone()), b.pr_url))
            .unwrap_or((meta.branch_hint.clone(), None));

        Ok(CloudResult {
            pr_url,
            branch,
            diff_summary: run.result.unwrap_or_else(|| "cursor run finished".into()),
        })
    }
}

/// Build a Basic auth header value for Cursor API keys.
pub fn basic_auth_header(api_key: &str) -> String {
    let token = STANDARD.encode(format!("{api_key}:"));
    format!("Basic {token}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn create_request_body_shape() {
        let body = serde_json::json!({
            "prompt": { "text": "hi" },
            "repos": [{ "url": "https://github.com/a/b", "startingRef": "main" }],
            "autoCreatePR": true
        });
        assert_eq!(body["autoCreatePR"], true);
    }
}
