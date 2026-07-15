// cargo-contention-bench — PERF-D6.
//
// Real workload: drive M parallel `cargo check -p <crate>` invocations against
// the same workspace. Measures wall-clock + peak RSS with vs without the
// G3 substrate-throttle (SUBSTRATE_THROTTLE_MAX=N caps concurrent cargo).
//
// This is the workload the G3 throttle was designed for. fake-forge is the
// wrong workload (no real contention) — see docs/perf/G3_PERF.md.
//
// Args (env):
//   CRATE        — crate to build (default: substrate-trace)
//   M            — concurrent cargo invocations per round (default: 8)
//   ROUNDS       — rounds to run (default: 3)
//   LABEL        — label for the BENCH_RESULT line (default: "default")
//   CLEAN        — set to "1" to `cargo clean -p <crate>` before each round
//                  (forces a real build). Default: "1".
//   TARGET_DIR   — override CARGO_TARGET_DIR (default: $WT/target)

use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use tokio::process::Command;

#[tokio::main(flavor = "multi_thread", worker_threads = 32)]
async fn main() -> anyhow::Result<()> {
    let m: usize = env::var("M").unwrap_or_else(|_| "8".into()).parse()?;
    let rounds: usize = env::var("ROUNDS").unwrap_or_else(|_| "3".into()).parse()?;
    let krate = env::var("CRATE").unwrap_or_else(|_| "substrate-trace".into());
    let label = env::var("LABEL").unwrap_or_else(|_| "default".into());
    let do_clean = env::var("CLEAN").unwrap_or_else(|_| "1".into()) == "1";

    // Worktree root = current directory (assumes the bench is run from $WT).
    let wt = std::env::current_dir()?;
    let target_dir: PathBuf = env::var("TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| wt.join("target"));

    eprintln!(
        "[cargo-contention-bench] label={label} M={m} rounds={rounds} crate={krate} clean={do_clean} target={}",
        target_dir.display()
    );

    if let Ok(cap) = env::var("SUBSTRATE_THROTTLE_MAX") {
        eprintln!("[cargo-contention-bench] SUBSTRATE_THROTTLE_MAX={cap}");
    } else {
        eprintln!("[cargo-contention-bench] SUBSTRATE_THROTTLE_MAX (unset)");
    }

    let mut total_wall = std::time::Duration::ZERO;
    let mut total_ok = 0usize;

    for r in 0..rounds {
        if do_clean {
            eprintln!("[round {r}] cargo clean -p {krate}");
            let st = Command::new("cargo")
                .args(["clean", "-p", &krate])
                .current_dir(&wt)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await?;
            if !st.success() {
                anyhow::bail!("cargo clean failed: {st}");
            }
        }

        let t0 = Instant::now();
        let mut handles = Vec::with_capacity(m);
        let krate_owned = krate.clone();
        let wt_owned = wt.clone();
        let target_dir_owned = target_dir.clone();
        for _ in 0..m {
            let krate_owned = krate_owned.clone();
            let wt_owned = wt_owned.clone();
            let target_dir_owned = target_dir_owned.clone();
            handles.push(tokio::spawn(async move {
                let mut child = Command::new("cargo")
                    .args(["check", "-p", &krate_owned, "--message-format=short"])
                    .current_dir(&wt_owned)
                    .env("CARGO_TARGET_DIR", &target_dir_owned)
                    .env("CARGO_BUILD_JOBS", "1")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                let exit = child.wait().await?;
                Ok::<_, anyhow::Error>(exit.code().unwrap_or(-1))
            }));
        }

        let mut ok = 0usize;
        for h in handles {
            match h.await {
                Ok(Ok(0)) => ok += 1,
                Ok(Ok(c)) => eprintln!("[round {r}] cargo exit={c}"),
                Ok(Err(e)) => eprintln!("[round {r}] cargo spawn err: {e}"),
                Err(e) => eprintln!("[round {r}] join err: {e}"),
            }
        }
        let wall = t0.elapsed();
        total_wall += wall;
        total_ok += ok;
        eprintln!(
            "[round {r}] M={m} ok={ok} wall_ms={:.2}",
            wall.as_secs_f64() * 1000.0
        );
    }

    let avg_wall_ms = total_wall.as_secs_f64() * 1000.0 / rounds as f64;
    let ops_per_s = (total_ok as f64) / total_wall.as_secs_f64();
    println!(
        "BENCH_RESULT label={label} M={m} rounds={rounds} ok={total_ok} \
         total_ms={:.2} avg_round_ms={:.2} ops_per_s={:.3}",
        total_wall.as_secs_f64() * 1000.0,
        avg_wall_ms,
        ops_per_s,
    );
    Ok(())
}