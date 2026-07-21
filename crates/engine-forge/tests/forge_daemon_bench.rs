//! D-domain F5 benchmark (2026-07-01): real substrate dispatch throughput.
//!
//! Bench surface: `ForgeEngine::run_simple` — the F5 fast path itself
//! (used by `list()` and `dump()`). Each call invokes
//! `fake-forge conversation dump <id>` which prints the canned DUMP_JSON
//! and exits. fake-forge exits in <1 ms when FAKE_FORGE_HANG is unset,
//! so this measures pure spawn overhead — exactly what the F5 wire is
//! optimising.
//!
//! Two arms, each run at M ∈ {8, 16, 32}:
//!   * direct:    FORGE_DAEMON unset → tokio Command::spawn
//!   * daemon:    FORGE_DAEMON=1 + FFI daemon alive → forge_daemon posix_spawn
//!
//! The daemon arm uses the same FFI symbols that the production wire uses
//! (`forge_daemon_start`, `forge_daemon_dispatch`), so the kernel cost we
//! measure here is the same cost the F5 wire saves in production.
//!
//! Run:
//!   cargo test -p engine-forge --release --features forge_daemon \
//!       --test forge_daemon_bench -- --nocapture --test-threads=1
//!
//! Env:
//!   F5_BENCH_M="8,16,32"  (default)
//!   F5_BENCH_SKIP_DAEMON=1  (skip the daemon arm; for fast re-runs)

use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_forge::ForgeEngine;
use substrate_core::ports::EnginePort;
use uuid::Uuid;

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

/// `bench-fake-forge` (sibling binary) prints a fixed conv-id and exits in
/// <1 ms regardless of argv. The F5 daemon builds the argv for the child,
/// so the daemon arm needs a binary that doesn't gate on specific flags.
fn bench_fake_forge_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let debug_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let path = debug_dir.join(format!("bench-fake-forge{suffix}"));

    if !path.exists() {
        let status = StdCommand::new(env!("CARGO"))
            .args(["build", "-p", "fake-forge", "--bin", "bench-fake-forge"])
            .status()
            .expect("failed to build bench-fake-forge");
        assert!(status.success(), "bench-fake-forge build failed");
    }
    path
}

/// Read RSS (MiB) of the current process. Best-effort telemetry.
fn current_rss_mib() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
        return Some(pages as f64 * 4.0 / 1024.0);
    }
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id().to_string();
        let out = StdCommand::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
            .ok()?;
        let kib: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        return Some(kib / 1024.0);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// A F5-shaped argv: `forge conversation dump <id>`. Matches what
/// `ForgeEngine::dump` builds internally; fake-forge prints the canned
/// DUMP_JSON in <1 ms.
fn dump_argv() -> Vec<String> {
    vec![
        "conversation".into(),
        "dump".into(),
        Uuid::new_v4().to_string(),
    ]
}

/// One dump() call → one F5 run_simple invocation. fake-forge returns in <1 ms.
async fn one_dump(engine: Arc<ForgeEngine>) -> Result<(), substrate_core::error::SubstrateError> {
    let _ = engine.dump(&dump_argv()[2]).await?;
    Ok(())
}

async fn run_m(engine: Arc<ForgeEngine>, m: usize) -> (Duration, usize) {
    let mut handles = Vec::with_capacity(m);
    let start = Instant::now();
    for _ in 0..m {
        let engine = engine.clone();
        handles.push(tokio::spawn(async move { one_dump(engine).await }));
    }
    let mut ok = 0;
    for h in handles {
        if let Ok(Ok(())) = h.await {
            ok += 1;
        }
    }
    (start.elapsed(), ok)
}

fn fmt_row(label: &str, m: usize, wall: Duration, ok: usize, rss: Option<f64>) -> String {
    let wall_ms = wall.as_millis() as f64;
    let agents_per_s = if wall_ms > 0.0 { (ok as f64) / (wall_ms / 1000.0) } else { 0.0 };
    let rss_s = rss
        .map(|mib| format!("{mib:7.1}"))
        .unwrap_or_else(|| "  n/a  ".into());
    let ok_pct = if m > 0 { (ok * 100) / m } else { 0 };
    format!(
        "{label:>9}  M={m:>2}  wall={wall_ms:>8.1} ms  agents/s={agents_per_s:>7.2}  ok={ok:>2}/{m:<2} ({ok_pct:>3}%)  rss={rss_s} MiB"
    )
}

#[cfg(feature = "forge_daemon")]
extern "C" {
    fn forge_daemon_start(path: *const std::os::raw::c_char) -> std::os::raw::c_int;
    fn forge_daemon_stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn f5_bench_m8_m16_m32() {
    std::env::set_var("FAKE_FORGE_HANG", "0");
    let ms_env = std::env::var("F5_BENCH_M").unwrap_or_else(|_| "8,16,32".into());
    let ms: Vec<usize> = ms_env
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if ms.is_empty() {
        panic!("F5_BENCH_M produced zero valid values: {ms_env:?}");
    }

    let bin = fake_forge_bin().to_string_lossy().into_owned();
    let skip_daemon = std::env::var("F5_BENCH_SKIP_DAEMON").ok().as_deref() == Some("1");

    println!("\nF5 bench (real substrate dispatch via run_simple)");
    println!(
        "fake-forge: {bin}    M values: {ms:?}    daemon arm: {}",
        if skip_daemon { "SKIPPED" } else { "ENABLED if F5 daemon alive" }
    );

    for &m in &ms {
        // ---- direct path (no env) ----
        std::env::remove_var("FORGE_DAEMON");
        let engine = Arc::new(ForgeEngine::with_bin(bin.clone()).with_timeout(Duration::from_secs(60)));
        let (wall, ok) = run_m(engine, m).await;
        println!("{}", fmt_row("direct", m, wall, ok, current_rss_mib()));

        if skip_daemon {
            continue;
        }

        // ---- daemon path ----
        #[cfg(feature = "forge_daemon")]
        {
            use std::ffi::CString;
            let sock = std::env::temp_dir().join(format!("f5-bench-{}-{}.sock", std::process::id(), m));
            let sock_s = sock.to_string_lossy().into_owned();
            let _ = std::fs::remove_file(&sock);
            let cpath = CString::new(sock_s.clone()).expect("nul in path");
            let rc = unsafe { forge_daemon_start(cpath.as_ptr()) };
            assert!(rc == 0, "forge_daemon_start for bench arm M={m} rc={rc}");
            std::env::set_var("FORGE_DAEMON", "1");
            std::env::set_var("FORGE_DAEMON_SOCKET", &sock_s);

            let engine = Arc::new(ForgeEngine::with_bin(bench_fake_forge_bin().to_string_lossy().into_owned()).with_timeout(Duration::from_secs(60)));
            let (wall, ok) = run_m(engine, m).await;
            println!("{}", fmt_row("daemon", m, wall, ok, current_rss_mib()));

            unsafe { forge_daemon_stop() };
            std::env::remove_var("FORGE_DAEMON");
            std::env::remove_var("FORGE_DAEMON_SOCKET");
            let _ = std::fs::remove_file(&sock);
        }

        #[cfg(not(feature = "forge_daemon"))]
        {
            println!("{:>9}  M={m:>2}  [skip: forge_daemon feature off]", "daemon");
        }
    }
}