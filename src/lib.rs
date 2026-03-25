//! sharecli - Shared CLI process manager for multi-project agent orchestration

pub mod commands;
pub mod config;
pub mod monitoring;
pub mod projects;
pub mod runtime;

use anyhow::Result;
use tracing::info;

pub fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sharecli=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .init();
}

pub fn run() -> Result<()> {
    info!("sharecli starting...");
    Ok(())
}
