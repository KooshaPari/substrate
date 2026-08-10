//! Contract tests and fake adapter for [`CloudDispatchPort`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod fake;

pub use fake::FakeCloudDispatch;

use std::time::Duration;
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

    // Some providers report terminal status before their diff endpoint is
    // readable.  Poll harvest briefly rather than turning that propagation
    // window into a flaky conformance failure.
    let mut result = None;
    for _ in 0..32 {
        match adapter.harvest(&handle).await {
            Ok(value) => {
                result = Some(value);
                break;
            }
            Err(SubstrateError::CloudDispatch(message))
                if message.contains("not ready for harvest") =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("conformance: harvest failed after Succeeded: {error}"),
        }
    }
    let result = result.expect("conformance: harvest did not become ready within timeout");
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
    use std::path::PathBuf;
    use std::process::Command as StdCommand;

    use cloud_codex::{CodexCloudConfig, CodexCloudDispatch};

    use super::*;

    /// Resolve the clean `fake-codex-cloud` binary, building it first if absent.
    fn resolve_fake_codex_bin() -> PathBuf {
        let exe = std::env::current_exe().expect("current_exe");
        let debug_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let clean = debug_dir.join(format!("fake-codex-cloud{suffix}"));
        if !clean.exists() {
            let status = StdCommand::new(env!("CARGO"))
                .args(["build", "-p", "fake-codex-cloud"])
                .status()
                .expect("spawn cargo build -p fake-codex-cloud");
            assert!(status.success(), "cargo build -p fake-codex-cloud failed");
        }
        clean
    }

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

    #[tokio::test]
    async fn codex_adapter_passes_conformance_with_fake_cli() {
        let bin = resolve_fake_codex_bin()
            .into_os_string()
            .into_string()
            .expect("fake-codex-cloud path");
        let adapter = CodexCloudDispatch::new(CodexCloudConfig {
            bin,
            env_id: "env-test".into(),
        });
        assert_cloud_dispatch_conformance(&adapter).await;
    }
}
