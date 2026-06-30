//! F5 (2026-06-30): forge-daemon opt-in drop-in for `engine_forge::run_simple`.
//!
//! Verifies the integration is structurally sound end-to-end:
//!   1. `FORGE_DAEMON=1` set + daemon not running → falls back to direct spawn,
//!      `start()` against `fake-forge` still succeeds and captures a conv id.
//!   2. `FORGE_DAEMON` unset → direct-spawn path, `start()` still succeeds.
//!
//! The two paths share all the engine-forge surface; the daemon fast-path
//! inside `run_simple` is a thin drop-in that only changes the syscall cost
//! (the Zig kqueue+posix_spawn hot core). Real perf numbers live in
//! `benchmarks/forge_daemon_bench/`.

use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::time::Duration;

use engine_forge::{ForgeEngine, DEFAULT_TIMEOUT_SECS};
use substrate_core::domain::{Task, TaskState};
use substrate_core::ports::EnginePort;
use uuid::Uuid;

/// Resolve `fake-forge` (mirrors helper in `spawn.rs`).
fn fake_forge_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let debug_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let clean = debug_dir.join(format!("fake-forge{suffix}"));

    if !clean.exists() {
        let status = StdCommand::new(env!("CARGO"))
            .args(["build", "-p", "fake-forge", "--bin", "fake-forge"])
            .status()
            .expect("failed to build fake-forge");
        assert!(status.success(), "fake-forge build failed");
    }
    clean
}

fn make_task() -> Task {
    Task {
        id: Uuid::new_v4(),
        prompt: "daemon-fastpath-test".into(),
        cwd: ".".into(),
        state: TaskState::Working,
        parent_task_id: None,
        requirement_id: None,
        epic_id: None,
    }
}

#[tokio::test]
async fn start_without_daemon_env_uses_command_spawn_path() {
    std::env::remove_var("FORGE_DAEMON");

    let engine = ForgeEngine::with_bin(fake_forge_bin().to_string_lossy().into_owned())
        .with_timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS));

    let task = make_task();
    let session = engine.start(&task).await.expect("start ok (direct path)");
    assert!(!session.conv_id.is_empty(), "conv_id captured");
}

#[tokio::test]
async fn start_with_daemon_env_falls_back_when_daemon_not_alive() {
    // FORGE_DAEMON=1 is set but we never started the daemon in this process,
    // so `ffi_is_running()` returns false → falls back to Command::spawn.
    // This is the SAFE default: env flag alone never breaks the existing
    // behaviour.
    std::env::set_var("FORGE_DAEMON", "1");

    let engine = ForgeEngine::with_bin(fake_forge_bin().to_string_lossy().into_owned())
        .with_timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS));

    let task = make_task();
    let session = engine
        .start(&task)
        .await
        .expect("start ok (fallback when daemon down)");
    assert!(!session.conv_id.is_empty(), "conv_id captured via fallback");

    std::env::remove_var("FORGE_DAEMON");
}