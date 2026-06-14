//! # transport-file
//!
//! A [`TransportPort`] backed by the filesystem. Each owner gets an
//! append-only JSONL mailbox (`<dir>/<owner>.jsonl`). [`claim`] acquires an
//! exclusive lease via `create_new` on a per-message lockfile (an atomic
//! filesystem CAS on Windows and Unix), then marks the message claimed in a
//! sidecar set, so at most one worker processes a given message.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use substrate_core::domain::{Mailbox, Message};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::TransportPort;
use uuid::Uuid;

/// File-backed transport rooted at a directory.
#[derive(Debug, Clone)]
pub struct FileTransport {
    root: PathBuf,
}

impl FileTransport {
    /// Create a transport rooted at `root` (created if absent).
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io)?;
        Ok(FileTransport { root })
    }

    fn mailbox_path(&self, owner: &str) -> PathBuf {
        self.root.join(format!("{owner}.jsonl"))
    }

    fn claim_lock_path(&self, owner: &str, id: &Uuid) -> PathBuf {
        self.root.join(format!("{owner}.{id}.claim"))
    }

    fn read_messages(&self, owner: &str) -> Result<Vec<Message>> {
        let path = self.mailbox_path(owner);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path).map_err(io)?;
        let mut out = Vec::new();
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            let m: Message = serde_json::from_str(line)?;
            out.push(m);
        }
        Ok(out)
    }
}

fn io(e: std::io::Error) -> SubstrateError {
    SubstrateError::Io(e.to_string())
}

#[async_trait]
impl TransportPort for FileTransport {
    async fn publish(&self, message: &Message) -> Result<()> {
        let path = self.mailbox_path(&message.to);
        let line = serde_json::to_string(message)?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(io)?;
        writeln!(f, "{line}").map_err(io)?;
        Ok(())
    }

    async fn subscribe(&self, owner: &str) -> Result<Vec<Message>> {
        self.read_messages(owner)
    }

    async fn claim(&self, owner: &str, message_id: &Uuid) -> Result<Message> {
        let msg = self
            .read_messages(owner)?
            .into_iter()
            .find(|m| &m.id == message_id)
            .ok_or_else(|| SubstrateError::NotFound(format!("message {message_id}")))?;

        // Atomic CAS: create_new fails if the lock already exists.
        let lock = self.claim_lock_path(owner, message_id);
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(mut f) => {
                let _ = write!(f, "claimed");
                Ok(msg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(
                SubstrateError::ClaimConflict(format!("message {message_id} already claimed")),
            ),
            Err(e) => Err(io(e)),
        }
    }

    async fn mailbox(&self, owner: &str) -> Result<Mailbox> {
        Ok(Mailbox {
            owner: owner.to_string(),
            messages: self.read_messages(owner)?,
        })
    }
}

/// Returns true if a per-message claim lock currently exists.
pub fn is_claimed(root: &Path, owner: &str, id: &Uuid) -> bool {
    root.join(format!("{owner}.{id}.claim")).exists()
}
