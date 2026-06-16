#![forbid(unsafe_code)]
//! SQLite-backed `MailboxStore` and `ClaimPort` implementations.

pub mod claim_store;
pub mod error;
mod schema;
pub mod store;

pub use claim_store::{bodies_are_near_duplicate, SqliteClaimStore};
pub use error::StoreError;
pub use store::SqliteMailboxStore;
