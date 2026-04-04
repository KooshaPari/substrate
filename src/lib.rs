//! sharecli - Shared CLI process manager
//!
//! Thin CLI wrapper around PhenoProc registry.
//!
//! Features:
//! - Process management via pheno-proc-core
//! - Command deduplication via pheno-proc-dedup
//! - Priority-based task queuing via pheno-proc-queue
//! - Multi-project orchestration

pub mod commands;
pub mod config;

// Re-export from PhenoProc registry
pub use pheno_proc_core::{ManagedProcess, ProcessInfo, ProcessPool};
pub use pheno_proc_dedup::{CommandLock, InMemoryLockAdapter, LockStatus};
pub use pheno_proc_queue::{InMemoryQueueAdapter, Priority, QueueItem, QueueStats, QueueStatus};

pub use anyhow::Result;
