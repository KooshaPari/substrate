//! Codex Cloud dispatch adapter stub (intentionally unimplemented).
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use substrate_core::cloud_dispatch_port::{
    CloudDispatchPort, CloudResult, CloudTaskHandle, CloudTaskStatus,
};
use substrate_core::error::{Result, SubstrateError};

/// Placeholder adapter — Codex Cloud dispatch is not wired yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexCloudDispatch;

impl CodexCloudDispatch {
    /// Create the stub adapter.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CloudDispatchPort for CodexCloudDispatch {
    async fn submit_task(
        &self,
        _repo: &str,
        _branch: &str,
        _prompt: &str,
    ) -> Result<CloudTaskHandle> {
        Err(SubstrateError::CloudDispatch(
            "cloud-codex: not implemented — use cloud-cursor or cloud-kilo".into(),
        ))
    }

    async fn poll_status(&self, _handle: &CloudTaskHandle) -> Result<CloudTaskStatus> {
        Err(SubstrateError::CloudDispatch(
            "cloud-codex: not implemented".into(),
        ))
    }

    async fn harvest(&self, _handle: &CloudTaskHandle) -> Result<CloudResult> {
        Err(SubstrateError::CloudDispatch(
            "cloud-codex: not implemented".into(),
        ))
    }
}
