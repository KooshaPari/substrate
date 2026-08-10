//! Network-free stand-in for `codex cloud` subcommands (exec/status/diff/apply).
//!
//! Recognised invocations when invoked as `fake-codex-cloud cloud …`:
//! * `cloud exec --env <id> [--branch <b>] <prompt>` → prints a task URL
//! * `cloud status <task_id>` → `[PENDING]` for first two polls, then terminal
//! * `cloud diff <task_id>` → canned unified diff on success tasks
//! * `cloud apply <task_id>` → prints apply success (no filesystem changes)

use std::fs;
use std::path::PathBuf;

fn state_path(task_id: &str) -> PathBuf {
    let base = std::env::temp_dir().join("fake-codex-cloud");
    let _ = fs::create_dir_all(&base);
    base.join(format!("{task_id}.polls"))
}

fn read_polls(task_id: &str) -> u32 {
    fs::read_to_string(state_path(task_id))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_polls(task_id: &str, polls: u32) {
    let _ = fs::write(state_path(task_id), polls.to_string());
}

fn is_failure_task(task_id: &str, prompt: &str) -> bool {
    task_id.contains("fail") || prompt.contains("trigger failure")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("cloud") {
        eprintln!("fake-codex-cloud: expected `cloud` subcommand, got: {args:?}");
        std::process::exit(2);
    }

    match args.get(1).map(String::as_str) {
        Some("exec") => run_exec(&args[2..]),
        Some("status") => run_status(&args[2..]),
        Some("diff") => run_diff(&args[2..]),
        Some("apply") => run_apply(&args[2..]),
        other => {
            eprintln!("fake-codex-cloud: unsupported cloud subcommand: {other:?}");
            std::process::exit(2);
        }
    }
}

fn run_exec(args: &[String]) {
    let mut env_id = String::new();
    let mut branch = "main".to_string();
    let mut prompt = String::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--env" => {
                i += 1;
                env_id = args.get(i).cloned().unwrap_or_default();
            }
            "--branch" => {
                i += 1;
                branch = args.get(i).cloned().unwrap_or_else(|| "main".into());
            }
            flag if flag.starts_with("--") => {}
            text => {
                prompt = text.to_string();
            }
        }
        i += 1;
    }

    if env_id.is_empty() {
        eprintln!("fake-codex-cloud: --env is required");
        std::process::exit(2);
    }
    if prompt.is_empty() {
        eprintln!("fake-codex-cloud: prompt is required");
        std::process::exit(2);
    }

    let suffix = if is_failure_task("", &prompt) {
        "fail"
    } else {
        "ok"
    };
    // Every invocation gets an isolated state file.  The conformance suites
    // execute the fake CLI in parallel, so a deterministic task id would let
    // one test consume another test's poll counter and make harvest flaky.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let safe_branch = branch.replace(['/', '\\'], "_");
    let task_id = format!(
        "task_i_fake_{suffix}_{safe_branch}_{}_{}",
        std::process::id(),
        nonce
    );
    write_polls(&task_id, 0);
    println!("https://chatgpt.com/codex/tasks/{task_id}");
    let _ = (env_id, prompt);
}

fn run_status(args: &[String]) {
    let task_id = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_default();
    if task_id.is_empty() {
        eprintln!("fake-codex-cloud: task id required");
        std::process::exit(2);
    }

    let polls = read_polls(&task_id) + 1;
    write_polls(&task_id, polls);

    if is_failure_task(&task_id, "") {
        if polls >= 3 {
            println!("[ERROR] scripted failure");
            println!("env • now");
            println!("no diff");
            std::process::exit(1);
        }
        println!("[PENDING] scripted failure");
        println!("env • now");
        println!("no diff");
        std::process::exit(1);
    }

    if polls >= 3 {
        println!("[READY] conformance probe");
        println!("env • now");
        println!("+2/-1 • 1 file");
        return;
    }

    println!("[PENDING] conformance probe");
    println!("env • now");
    println!("no diff");
    std::process::exit(1);
}

fn run_diff(args: &[String]) {
    let task_id = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_default();
    if is_failure_task(&task_id, "") {
        eprintln!("fake-codex-cloud: no diff for failed task");
        std::process::exit(1);
    }
    println!("--- a/README.md");
    println!("+++ b/README.md");
    println!("@@ -0,0 +1,2 @@");
    println!("+conformance");
    println!("+probe");
    let _ = task_id;
}

fn run_apply(args: &[String]) {
    let task_id = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_default();
    if is_failure_task(&task_id, "") {
        eprintln!("fake-codex-cloud: cannot apply failed task");
        std::process::exit(1);
    }
    println!("applied {task_id}");
}
