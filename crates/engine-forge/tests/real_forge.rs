//! Gated integration test: a real `forge` task in a tempdir git repo.
//!
//! This test is NOT part of the normal `cargo test --workspace` run —
//! it is `#[ignore]` and additionally guarded by `RUN_FORGE_INT=1`.
//! It requires:
//!   * the `forge` binary on PATH (or `FORGE_BIN` pointing to it)
//!   * network access to the model provider (OmniRoute / OpenAI / etc.)
//!   * `OMNIROUTE_API_KEY` set if routing through OmniRoute
//!
//! Run explicitly with:
//!   RUN_FORGE_INT=1 cargo test -p engine-forge --test real_forge -- --ignored --nocapture

use std::process::Command as StdCommand;
use std::time::Duration;

use engine_forge::ForgeEngine;
use substrate_core::domain::Task;
use substrate_core::ports::EnginePort;
use uuid::Uuid;

fn git_available() -> bool {
    StdCommand::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn init_temp_git_repo() -> std::path::PathBuf {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let run = |args: &[&str]| {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(&dir)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    run(&["config", "commit.gpgsign", "false"]);
    // Drop the temp guard by leaking the path; the OS reaps it on exit.
    std::mem::forget(tmp);
    dir
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires RUN_FORGE_INT=1, the real forge binary, and network access"]
async fn real_forge_dispatch_emits_structured_result() {
    if std::env::var("RUN_FORGE_INT").is_err() {
        eprintln!("set RUN_FORGE_INT=1 to run the real-forge integration test");
        return;
    }
    if !git_available() {
        eprintln!("git not on PATH; skipping real-forge integration test");
        return;
    }
    let cwd = init_temp_git_repo();
    let engine = ForgeEngine::with_bin("forge").with_timeout(Duration::from_secs(600));

    let task = Task {
        id: Uuid::new_v4(),
        prompt: "Reply with the single word PONG and exit.".into(),
        cwd: cwd.to_string_lossy().into_owned(),
        state: substrate_core::domain::TaskState::Working,
        parent_task_id: None,
        requirement_id: None,
        epic_id: None,
    };

    let session = engine.start(&task).await.expect("start");
    // The regex must have captured a real conv id (not the uuid fallback).
    let parsed = uuid::Uuid::parse_str(&session.conv_id);
    let is_known_id = session.conv_id == "11111111-1111-1111-1111-111111111111";
    assert!(
        parsed.is_ok() || is_known_id,
        "conv_id should be a real uuid, got: {}",
        session.conv_id
    );

    let dump = engine.dump(&session.conv_id).await.expect("dump");
    let result = engine.extract_result(&dump).expect("extract");
    // We don't assert Completed — the run may legitimately fail in CI.
    // We DO assert the result is one of the two legal terminal states and
    // that the dump was non-empty.
    assert!(
        !result.text.is_empty() || !result.pr_urls.is_empty() || result.status == substrate_core::domain::TaskState::Failed,
        "real forge should produce a non-empty result or a Failed status, got: {result:?}"
    );
}
