//! # substrate (driver-cli)
//!
//! The composition root: parses CLI args, wires the concrete adapters into
//! [`DispatchService`], and prints the [`StructuredResult`] as JSON.
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use engine_forge::ForgeEngine;
use store_file::FileStore;
use substrate_app::DispatchService;
use substrate_core::domain::Task;
use substrate_core::ports::DispatchApi;
use transport_file::FileTransport;

#[derive(Parser)]
#[command(
    name = "substrate",
    about = "Dispatch agent tasks over the substrate spine."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Dispatch a single task to an engine and print the structured result.
    Dispatch {
        /// Engine to use (Phase 0: only `forge`).
        #[arg(long, default_value = "forge")]
        engine: String,
        /// Use the bundled network-free fake forge.
        #[arg(long)]
        fake: bool,
        /// Working directory the engine runs in.
        #[arg(long)]
        cwd: String,
        /// The prompt to dispatch.
        prompt: String,
    },
}

/// Locate the bundled `fake-forge` binary next to the running executable.
fn fake_forge_path() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("resolve current exe")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("exe has no parent dir"))?;
    let name = if cfg!(windows) {
        "fake-forge.exe"
    } else {
        "fake-forge"
    };
    let candidate = dir.join(name);
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "fake-forge not found at {} (build the workspace first)",
            candidate.display()
        ))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Dispatch {
            engine,
            fake,
            cwd,
            prompt,
        } => {
            if engine != "forge" {
                return Err(anyhow!(
                    "Phase 0 supports only --engine forge, got {engine}"
                ));
            }
            if fake {
                let path = fake_forge_path()?;
                std::env::set_var("FORGE_BIN", path);
            }

            // State dirs live under cwd/.substrate so runs are self-contained.
            let state = PathBuf::from(&cwd).join(".substrate");
            let store = Arc::new(FileStore::new(state.join("store"))?);
            let transport = Arc::new(FileTransport::new(state.join("mailbox"))?);
            let forge = Arc::new(ForgeEngine::new());

            let svc = DispatchService::new(forge, store, transport);
            let task = Task::new(prompt, cwd);
            let result = svc
                .dispatch(task)
                .await
                .map_err(|e| anyhow!("dispatch failed: {e}"))?;

            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
    }
}
