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
    /// Event append rejected: expected sequence mismatch or duplicate seq.
    #[error("duplicate event seq: aggregate {aggregate_id} expected {expected}")]
    DuplicateEventSeq {
        /// Aggregate id.
        aggregate_id: String,
        /// Sequence the caller expected.
        expected: u64,
    },
}
