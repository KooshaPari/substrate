//! Cloud-dispatch CLI wiring: submit, poll, harvest.

use std::time::Duration;

use anyhow::{anyhow, Context};
use cloud_cursor::CursorCloudDispatch;
use cloud_kilo::KiloCloudDispatch;
use substrate_core::cloud_dispatch_port::{CloudDispatchPort, CloudTaskStatus};

/// Supported cloud dispatch platforms.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum CloudPlatform {
    /// Cursor Cloud Agents (REST API).
    Cursor,
    /// Kilo gateway + local git PR workflow.
    Kilo,
}

/// Run a cloud-dispatch task end-to-end and print JSON harvest result.
pub async fn run(
    platform: CloudPlatform,
    repo: &str,
    branch: &str,
    task: &str,
) -> anyhow::Result<()> {
    match platform {
        CloudPlatform::Cursor => {
            let adapter =
                CursorCloudDispatch::from_env().map_err(|e| anyhow!("cursor adapter: {e}"))?;
            run_with_adapter(&adapter, repo, branch, task).await
        }
        CloudPlatform::Kilo => {
            let adapter =
                KiloCloudDispatch::from_env().map_err(|e| anyhow!("kilo adapter: {e}"))?;
            run_with_adapter(&adapter, repo, branch, task).await
        }
    }
}

async fn run_with_adapter(
    adapter: &dyn CloudDispatchPort,
    repo: &str,
    branch: &str,
    task: &str,
) -> anyhow::Result<()> {
    let handle = adapter
        .submit_task(repo, branch, task)
        .await
        .context("submit_task")?;

    let mut delay = Duration::from_secs(2);
    loop {
        let status = adapter.poll_status(&handle).await.context("poll_status")?;
        match status {
            CloudTaskStatus::Queued | CloudTaskStatus::Running => {
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(2).min(Duration::from_secs(30));
            }
            CloudTaskStatus::Failed { message } => {
                return Err(anyhow!(
                    "cloud task failed: {}",
                    message.unwrap_or_else(|| "unknown".into())
                ));
            }
            CloudTaskStatus::Succeeded => break,
        }
    }

    let result = adapter.harvest(&handle).await.context("harvest")?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
