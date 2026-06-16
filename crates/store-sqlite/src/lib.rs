#![forbid(unsafe_code)]
//! SQLite-backed `MailboxStore` implementation.

pub mod error;
mod schema;
pub mod store;

pub use error::StoreError;
pub use store::SqliteMailboxStore;
