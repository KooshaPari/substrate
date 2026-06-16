//! Store error type.

use thiserror::Error;

/// Errors produced by the SQLite mailbox store.
#[derive(Debug, Error)]
pub enum StoreError {
    /// An underlying SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A JSON serialization/deserialization error.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Enqueue rejected: near-duplicate body.
    #[error("duplicate work: {0}")]
    Duplicate(String),
}
