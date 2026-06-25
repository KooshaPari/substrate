//! Cursor Cloud Agents REST adapter for [`CloudDispatchPort`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod api;

use async_trait::async_trait;
use substrate_core::cloud_dispatch_port::{
    CloudDispatchPort, CloudResult, CloudTaskHandle, CloudTaskStatus,
};

pub use api::{basic_auth_header, CursorCloudDispatch, DEFAULT_BASE_URL};

/// Map a Cursor run status string to [`CloudTaskStatus`].
pub fn map_run_status(raw: &str, error_message: Option<&str>) -> CloudTaskStatus {
    match raw {
        "CREATING" | "PENDING" => CloudTaskStatus::Queued,
        "RUNNING" => CloudTaskStatus::Running,
        "FINISHED" => CloudTaskStatus::Succeeded,
        "ERROR" | "CANCELLED" | "EXPIRED" => CloudTaskStatus::Failed {
            message: error_message.map(str::to_string),
        },
        _other => CloudTaskStatus::Running,
    }
}

#[async_trait]
impl CloudDispatchPort for CursorCloudDispatch {
    async fn submit_task(
        &self,
        repo: &str,
        branch: &str,
        prompt: &str,
    ) -> substrate_core::error::Result<CloudTaskHandle> {
        self.submit(repo, branch, prompt).await
    }

    async fn poll_status(
        &self,
        handle: &CloudTaskHandle,
    ) -> substrate_core::error::Result<CloudTaskStatus> {
        self.poll(handle).await
    }

    async fn harvest(
        &self,
        handle: &CloudTaskHandle,
    ) -> substrate_core::error::Result<CloudResult> {
        self.harvest_run(handle).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_cursor_run_statuses() {
        assert_eq!(map_run_status("CREATING", None), CloudTaskStatus::Queued);
        assert_eq!(map_run_status("RUNNING", None), CloudTaskStatus::Running);
        assert_eq!(map_run_status("FINISHED", None), CloudTaskStatus::Succeeded);
        assert_eq!(
            map_run_status("ERROR", Some("boom")),
            CloudTaskStatus::Failed {
                message: Some("boom".into())
            }
        );
    }

    #[test]
    fn auth_header_is_basic() {
        let header = basic_auth_header("test-key");
        assert!(header.starts_with("Basic "));
    }
}
