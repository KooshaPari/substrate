//! Error types for the supervisor crate.

use thiserror::Error;

/// Errors that can occur in the supervisor loop.
#[derive(Debug, Error)]
pub enum SupervisorError {
    /// Store operation failed.
    #[error("store error: {0}")]
    Store(String),
    /// Engine operation failed.
    #[error("engine error: {0}")]
    Engine(String),
    /// Mailbox wiring failed.
    #[error("mailbox wiring failed: {0}")]
    MailboxWire(String),
    /// Resume-400: reasoning_details not permitted.
    #[error("resume-400: reasoning_details not permitted")]
    Resume400,
    /// No unread messages in inbox.
    #[error("no unread messages")]
    NoMessages,
    /// Claim conflict: message already claimed by another worker.
    #[error("claim conflict: message already claimed")]
    ClaimConflict,
}
