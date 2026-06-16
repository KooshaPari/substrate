//! EnginePort end-to-end: spawn the fake-forge, capture the conv id via
//! the regex strategy, and assert the logfile was written.
//!
//! This test exercises the *real* `start()` path (process group, tee,
//! timeout) against the bundled fake-forge — no network, no live forge
//! binary. The conv-id regex strategy is the path that wins; the
//! list-diff fallback is unit-tested in `parse::tests`.

use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::time::Duration;

use engine_forge::{ForgeEngine, DEFAULT_TIMEOUT_SECS};
use substrate_core::domain::Task;
use substrate_core::ports::EnginePort;
use uuid::Uuid;

/// Resolve the clean `fake-forge` binary, building it first if absent.
///
/// Mirrors the helper in `driver-cli/tests/cli.rs` — `current_exe` lives at
/// `<target>/debug/deps/spawn-<hash>.exe`, so the clean bin is
/// `<target>/debug/fake-forge[.exe]`.
fn fake_forge_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let debug_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let clean = debug_dir.join(format!("fake-forge{suffix}"));

    if !clean.exists() {
        let status = StdCommand::new(env!("CARGO"))
            .args(["build", "-p", "fake-forge"])
            .status()
            .expect("spawn cargo build -p fake-forge");
        assert!(status.success(), "cargo build -p fake-forge failed");
    }
    clean
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_spawns_fake_forge_captures_conv_id_and_writes_logfile() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = ForgeEngine::with_bin(fake_forge_bin().to_string_lossy().into_owned())
        .with_log_root(tmp.path())
        .with_timeout(Duration::from_secs(30));

    let task = Task {
        id: Uuid::new_v4(),
        prompt: "echo hi".into(),
        cwd: tmp.path().to_string_lossy().into_owned(),
        state: substrate_core::domain::TaskState::Working,
        parent_task_id: None,
        requirement_id: None,
        epic_id: None,
    };

    let session = engine.start(&task).await.unwrap();
    assert_eq!(
        session.conv_id, "11111111-1111-1111-1111-111111111111",
        "regex strategy should pick up fake-forge's labelled id"
    );
    assert!(session.pid.is_some(), "spawn should yield a real pid");

    let logfile = session
        .logfile
        .as_deref()
        .expect("logfile path must be present");
    let log = std::fs::read_to_string(logfile).expect("logfile must exist after run");
    assert!(
        log.contains("conversation-id: 11111111-1111-1111-1111-111111111111"),
        "logfile should contain the conversation-id line, got:\n{log}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_default_timeout_matches_spec() {
    // The default timeout must be 1800s per the Phase 1 spec. This is a
    // regression guard against accidental shrinking.
    let engine = ForgeEngine::with_bin(fake_forge_bin().to_string_lossy().into_owned());
    assert_eq!(engine.timeout(), Duration::from_secs(DEFAULT_TIMEOUT_SECS));
    assert_eq!(DEFAULT_TIMEOUT_SECS, 1800);
}
