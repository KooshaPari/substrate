// f5-bench — M=8/16/32 micro-bench for engine-forge F5 forge-daemon fast path.
//
// Compares:
//   - direct path: FORGE_DAEMON unset → run_simple uses tokio::Command::spawn
//   - daemon path: FORGE_DAEMON=1 + daemon started → run_simple uses
//     DaemonDispatch::dispatch (Zig kqueue + posix_spawn C-ABI)
//
// Args (env): M (concurrent tasks per round), ITERS (total tasks),
//             LABEL (label only).
// Outputs a BENCH_RESULT line on stdout.

use std::env;
use std::sync::Arc;
use std::time::Instant;

use engine_forge::ForgeEngine;
use forge_daemon;
use substrate_core::domain::{Task, TaskState};
use substrate_core::ports::EnginePort;
use uuid::Uuid;

fn make_task() -> Task {
    Task {
        id: Uuid::new_v4(),
        prompt: "f5-bench".into(),
        cwd: ".".into(),
        state: TaskState::Working,
        parent_task_id: None,
        requirement_id: None,
        epic_id: None,
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 32)]
async fn main() -> anyhow::Result<()> {
    let m: usize = env::var("M").unwrap_or_else(|_| "8".into()).parse()?;
    let iters: usize = env::var("ITERS").unwrap_or_else(|_| "32".into()).parse()?;
    let rounds = iters / m.max(1);

    let forge_bin = env::var("FORGE_BIN")
        .unwrap_or_else(|_| "../target/debug/fake-forge".into());
    let label = env::var("LABEL").unwrap_or_else(|_| "unknown".into());

    eprintln!("[f5-bench] label={label} M={m} iters={iters} rounds={rounds} bin={forge_bin}");

    // If FORGE_DAEMON=1 is set, start the in-process Zig daemon so that
    // engine-forge::run_simple's F5 fast path engages (otherwise it falls
    // back to tokio::Command::spawn). Idempotent: safe to call when env is
    // unset.
    if env::var("FORGE_DAEMON").ok().as_deref() == Some("1") {
        forge_daemon::ffi_start(None).expect("forge_daemon start");
        eprintln!("[f5-bench] forge_daemon in-process started");
    }

    // Warm-up: avoid cold-start outliers.
    {
        let engine = ForgeEngine::with_bin(forge_bin.clone());
        let t = make_task();
        let _ = engine.start(&t).await.expect("warmup start");
    }

    let engine = Arc::new(ForgeEngine::with_bin(forge_bin));
    let t0 = Instant::now();
    let mut total_ok = 0usize;
    for r in 0..rounds {
        let mut handles = Vec::with_capacity(m);
        for _ in 0..m {
            let engine = engine.clone();
            handles.push(tokio::spawn(async move {
                let t = make_task();
                engine.start(&t).await.map(|_| ())
            }));
        }
        for h in handles {
            match h.await {
                Ok(Ok(_)) => total_ok += 1,
                Ok(Err(e)) => eprintln!("[f5-bench] start err: {e}"),
                Err(e) => eprintln!("[f5-bench] join err: {e}"),
            }
        }
        eprintln!("[f5-bench] round {} / {} done", r + 1, rounds);
    }
    let elapsed = t0.elapsed();

    let ops_per_s = (total_ok as f64) / elapsed.as_secs_f64();
    let per_op_ms = elapsed.as_secs_f64() * 1000.0 / (total_ok as f64).max(1.0);
    println!(
        "BENCH_RESULT label={label} M={m} iters={iters} ok={total_ok} \
         elapsed_ms={:.2} ops_per_s={:.2} per_op_ms={:.3}",
        elapsed.as_secs_f64() * 1000.0,
        ops_per_s,
        per_op_ms,
    );
    Ok(())
}