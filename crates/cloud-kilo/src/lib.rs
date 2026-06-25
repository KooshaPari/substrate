//! Kilo gateway-backed [`CloudDispatchPort`] (local git PR workflow).
//!
//! Kilo has no public cloud-agent REST API; this adapter uses the LLM gateway
//! plus local `git`/`gh` to materialize changes. See crate README.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod gateway;
mod worker;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use substrate_core::cloud_dispatch_port::{
    CloudDispatchPort, CloudResult, CloudTaskHandle, CloudTaskStatus,
};
use substrate_core::error::{Result, SubstrateError};
use uuid::Uuid;

pub use gateway::{KiloGatewayConfig, DEFAULT_GATEWAY_URL, DEFAULT_MODEL};
pub use worker::{parse_llm_payload, LlmDispatchPayload};

/// In-memory task record for async model-backed dispatch.
#[derive(Debug, Clone)]
struct TaskRecord {
    status: CloudTaskStatus,
    result: Option<CloudResult>,
}

/// Kilo model-backed cloud dispatch adapter.
#[derive(Debug, Clone)]
pub struct KiloCloudDispatch {
    config: KiloGatewayConfig,
    tasks: Arc<Mutex<HashMap<String, TaskRecord>>>,
}

impl KiloCloudDispatch {
    /// Build from environment (`KILO_API_KEY`, optional overrides).
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            config: KiloGatewayConfig::from_env()?,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Build with explicit gateway config.
    pub fn new(config: KiloGatewayConfig) -> Self {
        Self {
            config,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl CloudDispatchPort for KiloCloudDispatch {
    async fn submit_task(&self, repo: &str, branch: &str, prompt: &str) -> Result<CloudTaskHandle> {
        let id = format!("kilo-{}", Uuid::new_v4());
        self.tasks.lock().unwrap().insert(
            id.clone(),
            TaskRecord {
                status: CloudTaskStatus::Queued,
                result: None,
            },
        );

        let tasks = Arc::clone(&self.tasks);
        let config = self.config.clone();
        let repo = repo.to_string();
        let branch = branch.to_string();
        let prompt = prompt.to_string();
        let task_id = id.clone();

        tokio::spawn(async move {
            {
                let mut guard = tasks.lock().unwrap();
                if let Some(rec) = guard.get_mut(&task_id) {
                    rec.status = CloudTaskStatus::Running;
                }
            }

            let outcome = worker::run_dispatch(&config, &repo, &branch, &prompt).await;
            let mut guard = tasks.lock().unwrap();
            if let Some(rec) = guard.get_mut(&task_id) {
                match outcome {
                    Ok(result) => {
                        rec.status = CloudTaskStatus::Succeeded;
                        rec.result = Some(result);
                    }
                    Err(e) => {
                        rec.status = CloudTaskStatus::Failed {
                            message: Some(e.to_string()),
                        };
                    }
                }
            }
        });

        Ok(CloudTaskHandle { id })
    }

    async fn poll_status(&self, handle: &CloudTaskHandle) -> Result<CloudTaskStatus> {
        self.tasks
            .lock()
            .unwrap()
            .get(&handle.id)
            .map(|r| r.status.clone())
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))
    }

    async fn harvest(&self, handle: &CloudTaskHandle) -> Result<CloudResult> {
        let tasks = self.tasks.lock().unwrap();
        let rec = tasks
            .get(&handle.id)
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        match &rec.status {
            CloudTaskStatus::Succeeded => rec
                .result
                .clone()
                .ok_or_else(|| SubstrateError::CloudDispatch("missing harvest payload".into())),
            CloudTaskStatus::Failed { message } => Err(SubstrateError::CloudDispatch(
                message.clone().unwrap_or_else(|| "kilo task failed".into()),
            )),
            _ => Err(SubstrateError::CloudDispatch(
                "kilo task not ready for harvest".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_llm_json_payload() {
        let raw = r#"{"commit_message":"fix","pr_title":"Fix","pr_body":"body","diff_summary":"1 file","files":[]}"#;
        let parsed = parse_llm_payload(raw).unwrap();
        assert_eq!(parsed.commit_message, "fix");
        assert_eq!(parsed.diff_summary, "1 file");
    }
}
