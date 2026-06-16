//! Multi-provider argv construction for external coding-agent CLIs.
//!
//! Ported from `KooshaPari/thegent-dispatch` so substrate can own the surface.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// CLI flags for `substrate argv`.
pub mod cli;
/// Dry-run panel, JSON emit, and optional execution.
pub mod dispatch;
/// Provider-native argv builders.
pub mod provider;

pub use cli::ArgvCli;
