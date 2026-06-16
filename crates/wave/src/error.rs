//! Error types for the wave crate.

use thiserror::Error;

/// Errors that can occur in the wave runner.
#[derive(Debug, Error)]
pub enum WaveError {
    /// A lane's supervisor or engine errored.
    #[error("lane '{lane}' failed: {source}")]
    Lane {
        /// The lane name.
        lane: String,
        /// The underlying error.
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// A store operation failed.
    #[error("store error: {0}")]
    Store(String),
    /// A task would exceed the maximum allowed depth.
    #[error("max depth {max_depth} exceeded at depth {actual_depth} for task {task_id}")]
    DepthExceeded {
        /// The configured max depth.
        max_depth: usize,
        /// The observed depth.
        actual_depth: usize,
        /// The task that would have been created.
        task_id: uuid::Uuid,
    },
    /// Could not compute depth (e.g. cycle or missing parent).
    #[error("depth computation error for task {0}: {1}")]
    DepthCompute(uuid::Uuid, String),
}
