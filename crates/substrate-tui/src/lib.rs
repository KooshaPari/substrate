//! substrate-tui library surface.
//!
//! Re-exports [`run_dashboard`] so the `substrate dash` driver-cli subcommand
//! can launch the TUI without forking a subprocess.

pub mod app;
pub mod components;
pub mod config;
pub mod dispatch_client;
pub mod help;
pub mod proccompose;
pub mod runner;
pub mod statusbar;

pub use runner::run_dashboard;
