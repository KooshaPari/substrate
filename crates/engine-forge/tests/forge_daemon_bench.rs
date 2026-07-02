//! D-domain F5 benchmark (2026-07-01): real substrate dispatch throughput.
//!
//! Drives `ForgeEngine::start` with `fake-forge` as the binary. fake-forge
//! exits immediately when FAKE_FORGE_HANG is unset (default), so this is a
//! pure spawn-overhead benchmark — exactly what `run_simple` (and the
//! forge-daemon fast-path inside it) optimises for.
//!
//! M ∈ {8, 16, 32}; run each M twice:
//!   * `direct`:     FORGE_DAEMON unset → tokio Command::spawn path
//!   * `daemon`:     FORGE_DAEMON=1 + daemon running → forge_daemon posix_spawn path
//! (When the daemon is not running, the daemon arm transparently falls back
//! to direct spawn, which is the documented safety property; we don't
//! measure that here.)
//!
//! Metrics per cell:
//!   * wall_ms     — total wall clock for the M-concurrent start() batch
//!   * agents_per_s — M / (wall_ms / 1000)
//!   * rss_mib     — resident set of the substrate bench process after the run
//!   * ok          — count of successful starts
//!
//! Run with:
//!   cargo test -p engine-forge --release --test forge_daemon_bench \
//!     -- --nocapture --test-threads=1
//!
//! Optional env:
//!   F5_BENCH_M="8,16,32"  (default)
//!   F5_BENCH_SKIP_DAEMON=1  (skip the daemon arm; for fast re-runs)

use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_forge::{ForgeEngine, DEFAULT_TIMEOUT_SECS};
use substrate_core::domain::{Task, TaskState};
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

fn make_task(i: usize) -> Task {
    Task {
        id: Uuid::new_v4(),
        prompt: format!("bench-{}", i),
        cwd: ".".into(),
        state: TaskState::Working,
        parent_task_id: None,
        requirement_id: None,
        epic_id: None,
    }
}

/// Read RSS (MiB) of the current process. Best-effort telemetry; we do not
/// fail the bench if it cannot be read.
fn current_rss_mib() -> Option<f64> {
    // macOS: ps -o rss= gives resident size in KiB. Linux: /proc/self/statm.
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

async fn run_m(
    engine: Arc<ForgeEngine>,
    m: usize,
) -> (Duration, usize) {
    let mut handles = Vec::with_capacity(m);
    let start = Instant::now();
    for i in 0..m {
        let engine = engine.clone();
        handles.push(tokio::spawn(async move {
            let t = make_task(i);
            engine.start(&t).await.map(|s| s.conv_id)
        }));
    }
    let mut ok = 0;
    for h in handles {
        if let Ok(Ok(_)) = h.await {
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
    format!(
        "{label:>9}  M={m:>2}  wall={wall_ms:>8.1} ms  agents/s={agents_per_s:>7.2}  ok={ok:>2}/{m:<2}  rss={rss_s} MiB"
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn f5_bench_m8_m16_m32() {
    // Quiet the noisiest log lines during benches.
    std::env::set_var("FAKE_FORGE_HANG", "0");
    let _ = std::env::var("FAKE_FORGE_HANG"); // touch

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

    println!("\nF5 bench (real substrate dispatch through ForgeEngine::start)");
    println!(
        "fake-forge: {bin}    M values: {ms:?}    daemon arm: {}",
        if skip_daemon { "SKIPPED" } else { "ENABLED if F5 daemon alive" }
    );

    for &m in &ms {
        // ---- direct path (no env) ----
        std::env::remove_var("FORGE_DAEMON");
        let engine = Arc::new(
            ForgeEngine::with_bin(bin.clone()).with_timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
        );
        let (wall, ok) = run_m(engine, m).await;
        println!("{}", fmt_row("direct", m, wall, ok, current_rss_mib()));

        if skip_daemon {
            continue;
        }

        // ---- daemon path ----
        // The drop-in is opt-in via FORGE_DAEMON=1 AND ffi_is_running()==true.
        // ffi_is_running() reads an in-process static set by forge_daemon::start().
        // We start it ourselves here so the daemon arm actually fires.
        #[cfg(feature = "forge_daemon")]
        {
            use std::ffi::CString;
            let sock = std::env::temp_dir().join(format!(
                "f5-bench-{}-{}.sock",
                std::process::id(),
                m
            ));
            let sock_s = sock.to_string_lossy().into_owned();
            let _ = std::fs::remove_file(&sock);
            let cpath = CString::new(sock_s.clone()).expect("nul in path");
            let rc = unsafe { forge_daemon_ffi_start_for_test(cpath.as_ptr()) };
            assert!(rc == 0, "forge_daemon_start for bench arm M={m} rc={rc}");
            std::env::set_var("FORGE_DAEMON", "1");
            std::env::set_var("FORGE_DAEMON_SOCKET", &sock_s);

            let engine = Arc::new(
                ForgeEngine::with_bin(bin.clone())
                    .with_timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
            );
            let (wall, ok) = run_m(engine, m).await;
            println!("{}", fmt_row("daemon", m, wall, ok, current_rss_mib()));

            unsafe { forge_daemon_ffi_stop_for_test() };
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

// Extern decls so this bench compiles whether or not forge_daemon feature is
// enabled; the cfg-gated block above is the only caller.
#[cfg(feature = "forge_daemon")]
extern "C" {
    fn forge_daemon_start(path: *const std::os::raw::c_char) -> std::os::raw::c_int;
    fn forge_daemon_stop();
}

#[cfg(feature = "forge_daemon")]
unsafe fn forge_daemon_ffi_start_for_test(path: *const std::os::raw::c_char) -> i32 {
    forge_daemon_start(path)
}

#[cfg(feature = "forge_daemon")]
unsafe fn forge_daemon_ffi_stop_for_test() {
    forge_daemon_stop();
}