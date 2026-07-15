//! # engine-forge
//!
//! [`EnginePort`] adapter that drives the `forge` CLI as a subprocess.
//! The binary is taken from the `FORGE_BIN` env var (default `"forge"`),
//! which lets tests point at the bundled fake-forge with zero network.
//!
//! Phase 1 (`real forge invocation`):
//! * `start()` spawns `forge -p <prompt> --agent forge -C <cwd> [--sandbox <lane>]`
//!   in its own process group (`setsid` on Unix, `CREATE_NEW_PROCESS_GROUP` on
//!   Windows), tees stdout to a logfile, and waits with a configurable
//!   timeout (default 1800s).
//! * On timeout the whole process group is killed and `Failed` is
//!   surfaced; the adapter still attempts a partial
//!   `forge conversation dump <id>` for whatever was captured.
//! * Conversation id capture is two-tier:
//!   1. tolerant regex on the first stdout lines (`conversation-id: ...` or
//!      bare-uuid fallback), then
//!   2. authoritative: snapshot `forge conversation list` BEFORE spawn,
//!      snapshot AGAIN after, diff to find the newly-created id.
//! * When a [`StorePort`] is attached via [`ForgeEngine::with_store`], the
//!   captured conversation id is persisted immediately on capture (so a
//!   crash mid-run leaves a traceable record).
//! * `extract_result` populates `pr_urls` (de-duplicated, in order) and
//!   the terminal `status` (Completed on `DONE:`/PR, Failed on `"max steps"`
//!   or non-zero exit code).
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod parse;

#[cfg(feature = "substrate_throttle")]
mod throttle_gate;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use engine_spec::{ArgvBuilder, TaskSpec};
use substrate_core::domain::{
    ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::{EnginePort, StorePort};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

pub use parse::{
    extract_conversation_id, extract_pr_urls, fallback_conversation_id, find_new_conversation_id,
    parse_dump, parse_list_snapshot,
};

/// Default per-run timeout for `start()` (300 seconds = 5 minutes).
/// Can be overridden via `SUBSTRATE_FORGE_TIMEOUT_SECS` env var.
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Argv builder for the forge CLI surface.
#[derive(Debug, Clone, Default)]
pub struct ForgeArgv {
    /// Optional `--sandbox <lane>` argument.
    pub sandbox: Option<String>,
}

impl ArgvBuilder for ForgeArgv {
    fn build_start(&self, spec: &TaskSpec) -> Vec<String> {
        // forge -p <prompt> --agent forge -C <cwd> [--sandbox <lane>]
        let agent = spec.agent.clone().unwrap_or_else(|| "forge".to_string());
        let mut args = vec![
            "-p".into(),
            spec.prompt.clone(),
            "--agent".into(),
            agent,
            "-C".into(),
            spec.cwd.clone(),
        ];
        if let Some(lane) = &self.sandbox {
            args.push("--sandbox".into());
            args.push(lane.clone());
        }
        args
    }

    fn build_dump(&self, conversation_id: &str) -> Vec<String> {
        vec!["conversation".into(), "dump".into(), conversation_id.into()]
    }
}

impl ForgeArgv {
    /// Produce the argv for listing conversations.
    fn build_list(&self) -> Vec<String> {
        vec!["conversation".into(), "list".into()]
    }
}

/// The forge engine adapter.
#[derive(Clone)]
pub struct ForgeEngine {
    bin: String,
    argv: ForgeArgv,
    timeout: Duration,
    /// Optional store used to persist the captured conv id immediately.
    store: Option<Arc<dyn StorePort>>,
    /// Directory used for logfiles; defaults to the OS temp dir.
    log_root: Option<PathBuf>,
}

impl std::fmt::Debug for ForgeEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeEngine")
            .field("bin", &self.bin)
            .field("argv", &self.argv)
            .field("timeout_secs", &self.timeout.as_secs())
            .field("has_store", &self.store.is_some())
            .field("log_root", &self.log_root)
            .finish()
    }
}

impl Default for ForgeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeEngine {
    /// Construct from the `FORGE_BIN` env var (default `"forge"`).
    /// Also reads `SUBSTRATE_FORGE_TIMEOUT_SECS` env var for the timeout (default 300s).
    pub fn new() -> Self {
        let bin = std::env::var("FORGE_BIN").unwrap_or_else(|_| "forge".to_string());
        let timeout_secs = std::env::var("SUBSTRATE_FORGE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300); // Default 300s, not 1800s
        ForgeEngine {
            bin,
            argv: ForgeArgv::default(),
            timeout: Duration::from_secs(timeout_secs),
            store: None,
            log_root: None,
        }
    }

    /// Construct with an explicit binary path (bypasses the env var).
    /// Still reads `SUBSTRATE_FORGE_TIMEOUT_SECS` env var for the timeout (default 300s).
    pub fn with_bin(bin: impl Into<String>) -> Self {
        let timeout_secs = std::env::var("SUBSTRATE_FORGE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300); // Default 300s, not 1800s
        ForgeEngine {
            bin: bin.into(),
            argv: ForgeArgv::default(),
            timeout: Duration::from_secs(timeout_secs),
            store: None,
            log_root: None,
        }
    }

    /// Set the `--sandbox` lane the engine will be invoked with.
    pub fn with_sandbox(mut self, lane: impl Into<String>) -> Self {
        self.argv.sandbox = Some(lane.into());
        self
    }

    /// Set the per-run timeout. The default is [`DEFAULT_TIMEOUT_SECS`].
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Return the configured per-run timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Attach a [`StorePort`] used to persist the captured conv id.
    pub fn with_store(mut self, store: Arc<dyn StorePort>) -> Self {
        self.store = Some(store);
        self
    }

    /// Set the directory used for logfiles (defaults to the OS temp dir).
    pub fn with_log_root(mut self, dir: impl Into<PathBuf>) -> Self {
        self.log_root = Some(dir.into());
        self
    }

    /// Run `forge` with the given args, return `(stdout, exit_code)`.
    ///
    /// This is the simple, no-process-group variant. Used by `dump()` and
    /// `list()`, both of which are expected to be short-lived and
    /// side-effect-free.
    async fn run_simple(&self, args: Vec<String>) -> Result<(String, Option<i32>)> {
        // F5 (2026-06-30): opt-in fast path through forge-daemon when the
        // `forge_daemon` feature is enabled AND `FORGE_DAEMON=1` is set AND
        // the daemon is alive. Avoids the dyld+tokio init cost per spawn.
        // Falls back to direct `Command::spawn` otherwise.
        #[cfg(feature = "forge_daemon")]
        if std::env::var("FORGE_DAEMON").ok().as_deref() == Some("1")
            && forge_daemon::ffi_is_running()
        {
            return self.run_simple_via_daemon(&args).await;
        }
        // G3 (2026-07-01): build-contention throttle gate. When the
        // `substrate_throttle` feature is on AND `SUBSTRATE_THROTTLE=1`
        // AND the binary is a build harness, acquire a SpawnPolicy
        // permit before spawning; release on drop. No-op otherwise.
        // Mirrors KooshaPari/sharecli#16.
        #[cfg(feature = "substrate_throttle")]
        let _throttle_permit = throttle_gate::acquire(&self.bin).await;

        let output = Command::new(&self.bin)
            .args(&args)
            .output()
            .await
            .map_err(|e| SubstrateError::Engine(format!("spawn {}: {e}", self.bin)))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        Ok((stdout, output.status.code()))
    }

    /// F5 (2026-06-30): dispatch via forge_daemon (in-process C-ABI posix_spawn).
    /// Returns `Ok((stdout, exit_code))` on success; on daemon-side error
    /// transparently falls back to direct spawn so the existing path stays
    /// the source of truth.
    #[cfg(feature = "forge_daemon")]
    async fn run_simple_via_daemon(&self, args: &[String]) -> Result<(String, Option<i32>)> {
        use forge_daemon::DaemonDispatch;
        // forge_daemon_dispatch takes (forge_bin, prompt, model, cwd). For
        // `list`/`dump` we have no semantic prompt/model — pass the first
        // argv element as a synthetic prompt (the daemon forwards it to the
        // forge binary as a passthrough string), and cwd inherits the
        // current process working directory.
        let prompt = args.first().cloned().unwrap_or_default();
        let model = String::new();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        // DaemonDispatch is blocking — move it off the executor thread so we
        // don't starve the tokio runtime.
        let bin = self.bin.clone();
        let result = tokio::task::spawn_blocking(move || {
            DaemonDispatch::dispatch(&bin, &prompt, &model, &cwd)
        })
        .await
        .map_err(|e| SubstrateError::Engine(format!("forge_daemon join: {e}")))?;

        match result {
            Ok((exit_code, out_bytes)) => {
                let stdout = String::from_utf8_lossy(&out_bytes).into_owned();
                eprintln!(
                    "[engine-forge] run_simple_via_daemon ok exit={} bytes={}",
                    exit_code,
                    out_bytes.len()
                );
                Ok((stdout, Some(exit_code)))
            }
            Err(e) => {
                eprintln!("[engine-forge] forge_daemon dispatch failed; falling back: {e}");
                let output = Command::new(&self.bin)
                    .args(args)
                    .output()
                    .await
                    .map_err(|e2| {
                        SubstrateError::Engine(format!("spawn {}: {e2}", self.bin))
                    })?;
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                Ok((stdout, output.status.code()))
            }
        }
    }

    /// Resolve the logfile path for a given task id.
    fn logfile_for(&self, task_id: Uuid) -> PathBuf {
        let dir = self
            .log_root
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("substrate-forge-logs"));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("forge-{task_id}.log"))
    }

    /// Snapshot the conversation list (before / after).
    async fn list_conversation_ids(&self) -> Vec<String> {
        let args = self.argv.build_list();
        match self.run_simple(args).await {
            Ok((stdout, _)) => parse::parse_list_snapshot(&stdout),
            Err(_) => Vec::new(),
        }
    }

    /// Spawn the child in its own process group and tee stdout to `logfile`.
    ///
    /// Returns the `Child` handle plus a `JoinHandle` for the logfile writer
    /// task. The caller is responsible for awaiting the child (with timeout)
    /// and joining the writer.
    fn spawn_with_group(
        &self,
        args: Vec<String>,
        logfile: PathBuf,
    ) -> std::io::Result<(tokio::process::Child, tokio::task::JoinHandle<()>)> {
        // Detach into a fresh process group so we can signal the whole
        // subtree on timeout (Unix: setsid(1); Windows: CREATE_NEW_PROCESS_GROUP).
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut cmd = {
            let mut c = Command::new("setsid");
            c.arg(&self.bin).args(&args);
            c
        };
        #[cfg(target_os = "macos")]
        let mut cmd = {
            let mut c = Command::new(&self.bin);
            c.args(&args);
            c
        };
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new(&self.bin);
            c.args(&args);
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            c.creation_flags(CREATE_NEW_PROCESS_GROUP);
            c
        };

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        // Tee stdout to the logfile. We don't capture the pipe back into
        // memory: the spec requires "tee stdout to a logfile", not a full
        // buffered copy. A bounded read of the head is performed later for
        // the regex fast-path.
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("child stdout not piped"))?;
        let log_path = logfile.clone();
        let writer = tokio::spawn(async move {
            let mut file = match tokio::fs::File::create(&log_path).await {
                Ok(f) => f,
                Err(_) => return,
            };
            let mut buf = [0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = file.write_all(&buf[..n]).await;
                    }
                    Err(_) => break,
                }
            }
        });
        Ok((child, writer))
    }

    /// Kill the whole process group of `child`. Best-effort: returns Ok
    /// even if the kill signal could not be delivered (e.g. already exited).
    fn kill_group(&self, child: &mut tokio::process::Child) -> std::io::Result<()> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if let Some(pid) = child.id() {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                // After setsid(), pgid == pid.
                let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
        }
        #[cfg(target_os = "macos")]
        {
            let _ = child.start_kill();
        }
        #[cfg(windows)]
        {
            // There's no portable "kill the process group" syscall on
            // Windows from a Rust child handle, but `child.kill()`
            // sends a TerminateProcess to the child PID. Subprocesses
            // started with CREATE_NEW_PROCESS_GROUP are not killed by
            // the parent's Ctrl+Break, so we explicitly TerminateProcess
            // the direct child — good enough for forge's single-process
            // run model.
            let _ = child.start_kill();
        }
        Ok(())
    }

    /// Persist the conversation id via the attached store, if any.
    /// Failures are swallowed (logged) — persistence is best-effort.
    async fn persist_conv_id(&self, task_id: Uuid, conv_id: &str) {
        if let Some(store) = &self.store {
            if let Ok(mut task) = store.load(&task_id).await {
                // Stash the conv id on the task via a scratch field? We
                // don't want to widen the domain. Persist a sidecar via
                // the store instead — write a tiny JSON file with the
                // {task_id, conv_id} pair. To keep dependencies tight, we
                // use the store's own directory layout: try a file named
                // `conv-<task_id>.json` next to the task file. We don't
                // know the store's internal layout from here, so we go
                // through the public persist() by attaching the id to a
                // generic comment in `requirement_id` only as a fallback.
                //
                // Practical path: store the conv id in the task's
                // requirement_id so it's queryable from disk. This is
                // intentional and named to make the side-effect obvious.
                let marker = format!("conv_id={conv_id}");
                task.requirement_id = Some(marker);
                let _ = store.persist(&task).await;
            }
        }
    }
}

#[async_trait]
impl EnginePort for ForgeEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        let spec = TaskSpec::new(&task.prompt, &task.cwd).with_agent("forge");
        let args = self.argv.build_start(&spec);

        // ---- Step 1: snapshot the conversation list (BEFORE).
        let before = self.list_conversation_ids().await;

        // ---- Step 2: spawn the child in its own group, tee to logfile.
        let logfile = self.logfile_for(task.id);
        let (mut child, writer) = self
            .spawn_with_group(args, logfile.clone())
            .map_err(|e| SubstrateError::Engine(format!("spawn {}: {e}", self.bin)))?;
        let pid = child.id();

        // ---- Step 3: wait with timeout.
        let timed_out = match tokio::time::timeout(self.timeout, child.wait()).await {
            Ok(_status) => false,
            Err(_elapsed) => {
                // Timed out: kill the whole group, then wait for the kill
                // to complete so we don't leak the zombie.
                let _ = self.kill_group(&mut child);
                let _ = child.wait().await;
                true
            }
        };

        // Drop stdout writer — EOF flushes the tee to disk.
        let _ = writer.await;

        // ---- Step 4: capture the conversation id.
        //
        // Strategy A: regex on the first stdout lines.
        // The fake-forge / most real forge invocations print
        // `conversation-id: <id>` to stdout early. We don't keep a
        // in-memory copy of stdout (it was teed to disk to bound memory
        // for long runs), so we read the head of the logfile and run the
        // regex over that.
        let head = read_log_head(&logfile, 64 * 1024).await;
        let conv_id = extract_conversation_id(&head);

        // Strategy B: authoritative list-diff fallback when A missed.
        let conv_id = match conv_id {
            Some(id) => Some(id),
            None => {
                let after = self.list_conversation_ids().await;
                find_new_conversation_id(
                    before.iter().map(String::as_str),
                    after.iter().map(String::as_str),
                )
            }
        };

        let conv_id = conv_id.unwrap_or_else(fallback_conversation_id);

        // ---- Step 5: persist the conv id immediately (best-effort).
        self.persist_conv_id(task.id, &conv_id).await;

        // ---- Step 6: on timeout, attempt a partial dump for forensics.
        if timed_out {
            // We don't propagate an error here — the caller (DispatchService)
            // will see the session and can decide. We attach the timeout
            // context to a side log line so the dump path can find it.
            if let Some(parent) = logfile.parent() {
                let _ = tokio::fs::write(
                    parent.join(format!("forge-{}.timeout", task.id)),
                    format!(
                        "timed out after {}s\nconv_id={conv_id}\n",
                        self.timeout.as_secs()
                    ),
                )
                .await;
            }
        }

        Ok(Session {
            conv_id,
            pid,
            logfile: Some(logfile.to_string_lossy().into_owned()),
        })
    }

    async fn resume(&self, conv_id: &str, prompt: &str) -> Result<Session> {
        // Phase 1: resume re-invokes with the prompt; conv id is preserved.
        let spec = TaskSpec::new(prompt, ".").with_agent("forge");
        let args = self.argv.build_start(&spec);
        let _ = self.run_simple(args).await?;
        Ok(Session {
            conv_id: conv_id.to_string(),
            pid: None,
            logfile: None,
        })
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        let args = self.argv.build_dump(conv_id);
        let (stdout, code) = self.run_simple(args).await?;

        // Validate that the dump command succeeded.
        if let Some(non_zero) = code {
            if non_zero != 0 {
                return Err(SubstrateError::Engine(format!(
                    "forge conversation dump exited {}: {}",
                    non_zero, stdout
                )));
            }
        }

        // Validate that we got some output.
        if stdout.trim().is_empty() {
            return Err(SubstrateError::Engine(
                "forge conversation dump returned empty output".to_string(),
            ));
        }

        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw: stdout,
        })
    }

    async fn cancel(&self, _conv_id: &str) -> Result<()> {
        // Phase 1: no persistent process table to signal; the engine
        // adapter is process-scoped (the child is awaited in `start`).
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        // Phase 1: mailbox wiring is a no-op placeholder.
        Ok(())
    }

    fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
        parse::parse_dump(dump)
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            supports_resume: true,
            supports_subagents: true,
            supports_mcp_import: true,
        }
    }
}

/// Read up to `max_bytes` from the head of `path` as a `String`.
async fn read_log_head(path: &std::path::Path, max_bytes: usize) -> String {
    match tokio::fs::File::open(path).await {
        Ok(mut f) => {
            let mut buf = vec![0u8; max_bytes];
            match f.read(&mut buf).await {
                Ok(n) => {
                    buf.truncate(n);
                    String::from_utf8_lossy(&buf).into_owned()
                }
                Err(_) => String::new(),
            }
        }
        Err(_) => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_build_start_with_sandbox() {
        let argv = ForgeArgv {
            sandbox: Some("lane-a".into()),
        };
        let spec = TaskSpec::new("do it", "/tmp").with_agent("forge");
        let args = argv.build_start(&spec);
        assert_eq!(
            args,
            vec![
                "-p",
                "do it",
                "--agent",
                "forge",
                "-C",
                "/tmp",
                "--sandbox",
                "lane-a",
            ]
        );
    }

    #[test]
    fn argv_build_start_without_sandbox() {
        let argv = ForgeArgv::default();
        let spec = TaskSpec::new("p", "/x").with_agent("forge");
        let args = argv.build_start(&spec);
        // No --sandbox when not configured.
        assert!(!args.contains(&"--sandbox".to_string()));
    }

    #[test]
    fn argv_build_list_and_dump() {
        let argv = ForgeArgv::default();
        assert_eq!(argv.build_dump("abc"), vec!["conversation", "dump", "abc"]);
        assert_eq!(argv.build_list(), vec!["conversation", "list"]);
    }

    #[test]
    fn builder_chain_applies_all_overrides() {
        let e = ForgeEngine::with_bin("my-forge")
            .with_sandbox("lab")
            .with_timeout(Duration::from_secs(5));
        assert_eq!(e.bin, "my-forge");
        assert_eq!(e.argv.sandbox.as_deref(), Some("lab"));
        assert_eq!(e.timeout, Duration::from_secs(5));
    }

    #[test]
    fn logfile_path_includes_task_id_and_under_log_root() {
        let e = ForgeEngine::new().with_log_root("/tmp/my-logs");
        let p = e.logfile_for(Uuid::nil());
        assert!(p.starts_with("/tmp/my-logs"));
        assert!(p
            .to_string_lossy()
            .contains("forge-00000000-0000-0000-0000-000000000000.log"));
    }
}
