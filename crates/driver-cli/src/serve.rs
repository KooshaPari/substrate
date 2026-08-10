//! `serve` subcommand — start the substrate HTTP server with a lock-based
//! single-instance guard so no two actors can accidentally double-serve.
//!
//! # Flow
//!
//! 1. [`probe`] the serve-lock for `"substrate"`.
//! 2. [`decide`] what to do given the probe result and `--on-conflict` policy.
//! 3. On [`Decision::Serve`] or [`Decision::Replace`]: acquire the lock, bind
//!    the HTTP server, print the URL, and block until Ctrl+C.
//! 4. On [`Decision::Attach`]: print the existing server's URL and exit 0.
//! 5. On [`Decision::Abort`]: print the reason and exit with a non-zero code.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use driver_http::{serve as http_serve, HttpConfig};
use substrate_serve_lock::{decide, probe, Decision, OnConflict, ServeLock, ServeState};
use tokio::signal;

/// Arguments for the `serve` subcommand.
#[derive(clap::Args, Debug)]
#[command(next_help_heading = "SERVE")]
pub struct ServeArgs {
    /// Socket address to listen on.
    #[arg(long, default_value = "127.0.0.1:8080", value_name = "ADDR")]
    pub bind: SocketAddr,

    /// What to do when another substrate server is already running.
    ///
    /// - `abort`   (default) — print the conflict and exit non-zero.
    /// - `attach`  — print the running server's URL and exit 0.
    /// - `replace` — take over the lock and restart the server.
    #[arg(long, value_enum, default_value = "abort", value_name = "POLICY")]
    pub on_conflict: OnConflictArg,

    /// Root directory for `.substrate` state (store, mailbox, sqlite).
    /// Defaults to `.substrate` in the current directory.
    #[arg(long, value_name = "DIR")]
    pub state_dir: Option<PathBuf>,
}

/// Clap-facing mirror of [`OnConflict`] (clap needs `ValueEnum`; the core type
/// is in a no-clap crate so we keep the coupling here).
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnConflictArg {
    Abort,
    Attach,
    Replace,
}

impl From<OnConflictArg> for OnConflict {
    fn from(a: OnConflictArg) -> Self {
        match a {
            OnConflictArg::Abort => OnConflict::Abort,
            OnConflictArg::Attach => OnConflict::Attach,
            OnConflictArg::Replace => OnConflict::Replace,
        }
    }
}

/// Entry point for `substrate serve`.
pub async fn run(args: ServeArgs) -> Result<()> {
    let bind_url = format!("http://{}", args.bind);
    let policy: OnConflict = args.on_conflict.into();

    let state = probe("substrate").context("probe substrate serve-lock")?;

    match decide(&state, policy) {
        Decision::Attach => {
            // A live server is already running — report its URL and exit clean.
            let url = match &state {
                ServeState::Running { info, .. } => info.url.clone(),
                ServeState::Free => bind_url.clone(), // unreachable in practice
            };
            eprintln!("substrate serve: already running at {url} (attach)");
            return Ok(());
        }
        Decision::Abort => {
            let url = match &state {
                ServeState::Running { info, .. } => info.url.clone(),
                ServeState::Free => String::new(),
            };
            eprintln!("substrate serve: another server is running at {url}; refusing (--on-conflict abort)");
            process::exit(1);
        }
        Decision::Replace => {
            eprintln!(
                "substrate serve: replacing existing server at {bind_url} (--on-conflict replace)"
            );
            // Fall through to acquire + serve below; ServeLock::try_acquire
            // will take over the stale/replaced pidfile.
        }
        Decision::Serve => {
            // No conflict — proceed.
        }
    }

    // Acquire the serve-lock. If another process snuck in between probe and now,
    // fail fast rather than double-serving.
    let _lock = ServeLock::try_acquire("substrate", &bind_url)
        .context("acquire substrate serve-lock")?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "substrate serve: failed to acquire lock — another server started concurrently"
            )
        })?;

    let state_dir = args
        .state_dir
        .unwrap_or_else(|| PathBuf::from(".substrate"));

    let config = HttpConfig {
        bind: args.bind,
        state_dir,
        auth_token: std::env::var("SUBSTRATE_HTTP_AUTH_TOKEN").ok(),
    };

    eprintln!("substrate serve: listening on {bind_url}");

    // Run the HTTP server until Ctrl+C; the ServeLock Drop cleans up the pidfile.
    tokio::select! {
        result = http_serve(config) => {
            result.context("substrate HTTP server exited with error")?;
        }
        _ = shutdown_signal() => {
            eprintln!("substrate serve: received shutdown signal, stopping");
        }
    }

    Ok(())
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
