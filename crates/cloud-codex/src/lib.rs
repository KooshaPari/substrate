//! Codex Cloud CLI adapter for [`CloudDispatchPort`].
//!
//! Drives the experimental `codex cloud` surface:
//! `exec` to submit, `status` to poll, `diff`/`apply` to harvest.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod cli;

use async_trait::async_trait;
use substrate_core::cloud_dispatch_port::{
    CloudDispatchPort, CloudResult, CloudTaskHandle, CloudTaskStatus,
};

pub use cli::{
    map_codex_status, parse_status_label, parse_summary_line, parse_task_id_from_output,
    strip_ansi, summarize_diff, CodexCloudConfig, CodexCloudDispatch, CodexCommandOutput,
    CodexCommandRunner, TokioCodexRunner, ENV_CLOUD_ENV_ID,
};

#[async_trait]
impl CloudDispatchPort for CodexCloudDispatch {
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
        self.harvest_task(handle).await
    }
}

#[cfg(test)]
mod conformance_tests {
    use std::path::PathBuf;
    use std::process::Command as StdCommand;

    use cloud_dispatch_conformance::assert_cloud_dispatch_conformance;

    use super::*;

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
    async fn codex_cloud_dispatch_conformance_with_fake_cli() {
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
