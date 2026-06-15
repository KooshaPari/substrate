#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # supervisor
//!
//! Supervisor loop managing one teammate lane: spawn, pump, restart, resume-400 fallback.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use supervisor::{Supervisor, LaneConfig, FakeEngine, FakeResponse};
//! use store_sqlite::SqliteMailboxStore;
//!
//! let engine = Arc::new(FakeEngine::new());
//! let store = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());
//! let config = LaneConfig::new("team-1", "agent-a");
//! let mut sup = Supervisor::new(engine, store, config);
//! sup.spawn("write some tests").await.unwrap();
//! ```

pub mod error;
pub mod fake_engine;
pub mod supervisor;

pub use error::SupervisorError;
pub use fake_engine::{FakeEngine, FakeResponse};
pub use supervisor::{LaneConfig, Supervisor};
