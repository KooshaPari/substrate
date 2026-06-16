//! # store-file
//!
//! A [`StorePort`] backed by JSON files: `<dir>/task-<id>.json` and
//! `<dir>/result-<id>.json`. [`claim_atomic`] leases a task via `create_new`
//! on a lockfile (atomic filesystem CAS) and advances `Submitted -> Working`,
//! so at most one worker may claim a given task.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fs::{self, OpenOptions};
use std::path::PathBuf;

use async_trait::async_trait;
use substrate_core::domain::{StructuredResult, Task, TaskState};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::StorePort;
use uuid::Uuid;

/// File-backed store rooted at a directory.
#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    /// Create a store rooted at `root` (created if absent).
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io)?;
        Ok(FileStore { root })
    }

    fn task_path(&self, id: &Uuid) -> PathBuf {
        self.root.join(format!("task-{id}.json"))
    }

    fn result_path(&self, id: &Uuid) -> PathBuf {
        self.root.join(format!("result-{id}.json"))
    }

    fn claim_lock_path(&self, id: &Uuid) -> PathBuf {
        self.root.join(format!("task-{id}.claim"))
    }
}

fn io(e: std::io::Error) -> SubstrateError {
    SubstrateError::Io(e.to_string())
}

#[async_trait]
impl StorePort for FileStore {
    async fn persist(&self, task: &Task) -> Result<()> {
        let json = serde_json::to_string_pretty(task)?;
        fs::write(self.task_path(&task.id), json).map_err(io)?;
        Ok(())
    }

    async fn load(&self, id: &Uuid) -> Result<Task> {
        let path = self.task_path(id);
        let raw = fs::read_to_string(&path)
            .map_err(|_| SubstrateError::NotFound(format!("task {id}")))?;
        Ok(serde_json::from_str(&raw)?)
    }

    async fn persist_result(&self, task_id: &Uuid, result: &StructuredResult) -> Result<()> {
        let json = serde_json::to_string_pretty(result)?;
        fs::write(self.result_path(task_id), json).map_err(io)?;
        Ok(())
    }

    async fn claim_atomic(&self, id: &Uuid) -> Result<Task> {
        // Atomic CAS lease: create_new fails if the lock already exists.
        let lock = self.claim_lock_path(id);
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(SubstrateError::ClaimConflict(format!(
                    "task {id} already claimed"
                )));
            }
            Err(e) => return Err(io(e)),
        }

        let mut task = self.load(id).await?;
        // CAS on lifecycle state: only a Submitted task is claimable.
        if task.state != TaskState::Submitted {
            return Err(SubstrateError::ClaimConflict(format!(
                "task {id} not claimable from {:?}",
                task.state
            )));
        }
        task.advance(TaskState::Working)?;
        self.persist(&task).await?;
        Ok(task)
    }
}
