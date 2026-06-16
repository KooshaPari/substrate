//! # substrate-core
//!
//! The hexagonal **core**: pure domain contracts and the port traits that
//! adapters implement. This crate depends on nothing but `serde`,
//! `serde_json`, `thiserror`, `uuid`, and `async-trait` (required to express
//! async port traits). It MUST NOT depend on any adapter crate
//! (`engine-*`, `transport-*`, `store-*`, `driver-*`, `*-adapter`); this is
//! enforced by `crates/arch-test`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod claim_port;
pub mod domain;
pub mod error;
pub mod mailbox_port;
pub mod ports;
pub mod schedule_port;
pub mod trace;
pub mod workflow_port;

pub use claim_port::{ClaimPort, WorkItem, WorkItemState};
pub use error::{Result, SubstrateError};
pub use mailbox_port::MailboxStore;
pub use schedule_port::{ScheduleInstant, SchedulePort, ScheduleTrigger, Weekday};
pub use trace::{TaskCompleted, TaskFailed, TaskRegistered, TracePort};
pub use workflow_port::{Workflow, WorkflowEdge, WorkflowNode, WorkflowPort};
