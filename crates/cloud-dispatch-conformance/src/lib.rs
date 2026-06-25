//! Contract tests and fake adapter for [`CloudDispatchPort`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod fake;

pub use fake::FakeCloudDispatch;

use substrate_core::cloud_dispatch_port::{CloudDispatchPort, CloudResult, CloudTaskStatus};
use substrate_core::error::SubstrateError;

/// Run the full cloud-dispatch conformance suite against `adapter`.
///
/// Covers the happy path (submit → poll → harvest) and a failed-task path.
pub async fn assert_cloud_dispatch_conformance(adapter: &dyn CloudDispatchPort) {
    assert_happy_path(adapter).await;
    assert_failed_task_path(adapter).await;
}

/// Happy path: submit, poll to success, harvest PR metadata.
pub async fn assert_happy_path(adapter: &dyn CloudDispatchPort) {
    let handle = adapter
        .submit_task(
            "https://github.com/example/repo",
            "main",
            "add conformance probe",
        )
        .await
        .expect("conformance: submit_task must succeed");
    assert!(
        !handle.id.is_empty(),
        "conformance: handle.id must be non-empty"
    );

    let mut status = adapter
        .poll_status(&handle)
        .await
        .expect("conformance: poll_status must succeed");
    let mut polls = 0;
    while matches!(status, CloudTaskStatus::Queued | CloudTaskStatus::Running) {
        polls += 1;
        assert!(
            polls <= 32,
            "conformance: poll_status did not reach terminal state within 32 polls, last={status:?}"
        );
        status = adapter
            .poll_status(&handle)
            .await
            .expect("conformance: poll_status must succeed");
    }

    assert_eq!(
        status,
        CloudTaskStatus::Succeeded,
        "conformance: happy-path task must succeed"
    );

    let result = adapter
        .harvest(&handle)
        .await
        .expect("conformance: harvest must succeed after Succeeded");
    assert!(
        !result.branch.is_empty(),
        "conformance: harvest.branch must be non-empty"
    );
    assert!(
        !result.diff_summary.is_empty(),
        "conformance: harvest.diff_summary must be non-empty"
    );
}

/// Failed path: second submit uses scripted failure; harvest must error.
pub async fn assert_failed_task_path(adapter: &dyn CloudDispatchPort) {
    let handle = adapter
        .submit_task("https://github.com/example/repo", "main", "trigger failure")
        .await
        .expect("conformance: failed-path submit_task must succeed");

    let mut status = adapter
        .poll_status(&handle)
        .await
        .expect("conformance: failed-path poll_status must succeed");
    let mut polls = 0;
    while matches!(status, CloudTaskStatus::Queued | CloudTaskStatus::Running) {
        polls += 1;
        assert!(
            polls <= 32,
            "conformance: failed-path poll stuck at {status:?}"
        );
        status = adapter
            .poll_status(&handle)
            .await
            .expect("conformance: failed-path poll_status must succeed");
    }

    assert!(
        matches!(status, CloudTaskStatus::Failed { .. }),
        "conformance: failed-path task must end Failed, got {status:?}"
    );

    let harvest = adapter.harvest(&handle).await;
    assert!(
        harvest.is_err(),
        "conformance: harvest must fail when task Failed"
    );
}

/// Validate that a harvested [`CloudResult`] has the expected happy-path shape.
pub fn assert_valid_cloud_result(result: &CloudResult) {
    assert!(!result.branch.is_empty(), "branch must be non-empty");
    assert!(
        !result.diff_summary.is_empty(),
        "diff_summary must be non-empty"
    );
}

/// Map harvest errors for failed tasks — adapters should reject harvest.
pub fn expect_harvest_not_ready(err: SubstrateError) -> bool {
    matches!(
        err,
        SubstrateError::CloudDispatch(_)
            | SubstrateError::InvalidTransition { .. }
            | SubstrateError::Other(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_adapter_passes_conformance() {
        let fake = FakeCloudDispatch::new();
        assert_cloud_dispatch_conformance(&fake).await;
    }

    #[tokio::test]
    async fn fake_happy_path_alone() {
        let fake = FakeCloudDispatch::new();
        assert_happy_path(&fake).await;
    }

    #[tokio::test]
    async fn fake_failed_path_alone() {
        let fake = FakeCloudDispatch::new();
        assert_failed_task_path(&fake).await;
    }
}
