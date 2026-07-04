//! Substrate TUI — terminal UI dashboard for the substrate dispatch surface.
//!
//! This is a work-in-progress. The main entry point, event loop, and tab
//! switching will be completed in a follow-up. For now this file exists
//! so that the module-level types compile and pass `cargo check`.

mod app;
mod components;
mod config;
mod dispatch_client;
mod help;
mod proccompose;
mod statusbar;

fn main() {
    println!("substrate-tui: dashboard binary (WIP)");
    println!("modules: app, config, dispatch_client, components, proccompose, statusbar, help");
}
