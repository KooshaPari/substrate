//! Scripted fake [`CloudDispatchPort`] for offline contract tests.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use substrate_core::cloud_dispatch_port::{
    CloudDispatchPort, CloudResult, CloudTaskHandle, CloudTaskStatus,
};
use substrate_core::error::{Result, SubstrateError};

/// Outcome scripted for the next submitted task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Success,
    Fail,
}

#[derive(Debug, Clone)]
struct TaskRecord {
    polls: u32,
    terminal: Option<CloudTaskStatus>,
    result: Option<CloudResult>,
}

/// Offline fake implementing the full cloud-dispatch lifecycle.
#[derive(Clone, Default)]
pub struct FakeCloudDispatch {
    tasks: Arc<Mutex<Vec<TaskRecord>>>,
}

impl FakeCloudDispatch {
    /// Create a fake adapter for offline contract tests.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CloudDispatchPort for FakeCloudDispatch {
    async fn submit_task(
        &self,
        _repo: &str,
        _branch: &str,
        prompt: &str,
    ) -> Result<CloudTaskHandle> {
        let script = if prompt.contains("trigger failure") {
            Script::Fail
        } else {
            Script::Success
        };

        let mut tasks = self.tasks.lock().unwrap();
        let idx = tasks.len();
        let (terminal, result) = match script {
            Script::Success => (
                Some(CloudTaskStatus::Succeeded),
                Some(CloudResult {
                    pr_url: Some(format!("https://github.com/example/repo/pull/{idx}")),
                    branch: format!("fake/cloud-{idx}"),
                    diff_summary: format!("fake diff for: {prompt}"),
                }),
            ),
            Script::Fail => (
                Some(CloudTaskStatus::Failed {
                    message: Some("scripted failure".into()),
                }),
                None,
            ),
        };

        tasks.push(TaskRecord {
            polls: 0,
            terminal,
            result,
        });

        Ok(CloudTaskHandle {
            id: format!("fake-{idx}"),
        })
    }

    async fn poll_status(&self, handle: &CloudTaskHandle) -> Result<CloudTaskStatus> {
        let mut tasks = self.tasks.lock().unwrap();
        let idx: usize = handle
            .id
            .strip_prefix("fake-")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;
        let task = tasks
            .get_mut(idx)
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        task.polls += 1;
        match task.polls {
            1 => Ok(CloudTaskStatus::Queued),
            2 => Ok(CloudTaskStatus::Running),
            _ => task
                .terminal
                .clone()
                .ok_or_else(|| SubstrateError::CloudDispatch("no terminal state".into())),
        }
    }

    async fn harvest(&self, handle: &CloudTaskHandle) -> Result<CloudResult> {
        let tasks = self.tasks.lock().unwrap();
        let idx: usize = handle
            .id
            .strip_prefix("fake-")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;
        let task = tasks
            .get(idx)
            .ok_or_else(|| SubstrateError::NotFound(handle.id.clone()))?;

        match &task.terminal {
            Some(CloudTaskStatus::Succeeded) => task
                .result
                .clone()
                .ok_or_else(|| SubstrateError::CloudDispatch("missing harvest payload".into())),
            Some(CloudTaskStatus::Failed { message }) => Err(SubstrateError::CloudDispatch(
                message.clone().unwrap_or_else(|| "task failed".into()),
            )),
            _ => Err(SubstrateError::CloudDispatch(
                "task not ready for harvest".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ids_are_unique() {
        let fake = FakeCloudDispatch::new();
        let h1 = fake.submit_task("r", "b", "p1").await.unwrap();
        let h2 = fake.submit_task("r", "b", "p2").await.unwrap();
        assert_ne!(h1.id, h2.id);
    }
}
