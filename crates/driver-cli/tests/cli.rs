//! Integration: `substrate dispatch --engine forge` runs with zero network,
//! driven by the bundled fake-forge (resolved via `FORGE_BIN`).

use std::path::PathBuf;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;

/// Resolve the clean `fake-forge` binary, building it first if absent.
///
/// `current_exe` lives at `<target>/debug/deps/cli-<hash>.exe`, so the clean
/// bin is `<target>/debug/fake-forge[.exe]`. The hashed copy in `deps/` is the
/// libtest harness, not the real program, so we never use it.
fn fake_forge_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let debug_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let clean = debug_dir.join(format!("fake-forge{suffix}"));

    if !clean.exists() {
        // Build the bin into the same target dir (honors any CARGO_TARGET_DIR).
        let status = StdCommand::new(env!("CARGO"))
            .args(["build", "-p", "fake-forge"])
            .status()
            .expect("spawn cargo build -p fake-forge");
        assert!(status.success(), "cargo build -p fake-forge failed");
    }
    clean
}

#[test]
fn dispatch_fake_forge_emits_completed_json() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_forge_bin();
    assert!(
        fake.exists(),
        "fake-forge binary missing at {} after build",
        fake.display()
    );

    let mut cmd = Command::cargo_bin("substrate").unwrap();
    cmd.env("FORGE_BIN", &fake).args([
        "dispatch",
        "--engine",
        "forge",
        "--cwd",
        tmp.path().to_str().unwrap(),
        "echo hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"completed\""))
        .stdout(predicate::str::contains("DONE: printed hi"));
}
