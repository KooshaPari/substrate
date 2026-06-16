#![forbid(unsafe_code)]
//! SQLite-backed `MailboxStore` and `ClaimPort` implementations.

pub mod claim_store;
pub mod error;
pub mod event_store;
pub mod memory_store;
mod schema;
pub mod store;

pub use claim_store::{bodies_are_near_duplicate, SqliteClaimStore};
pub use error::StoreError;
pub use event_store::SqliteEventStore;
pub use memory_store::SqliteMemoryStore;
pub use store::SqliteMailboxStore;
