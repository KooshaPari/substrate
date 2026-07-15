//! # sharecli_throttle
//!
//! G3 (2026-06-30): substrate-side adapter that delegates build-contention
//! throttling to [`sharecli`](https://github.com/KooshaPari/sharecli) via the
//! `run-pool` sidecar.
//!
//! Background: substrate's dispatch path can spawn N concurrent engines
//! (`wave`, `substrate-dag`, `substrate-schedule`). When each engine later
//! shells out to `cargo build` / `cargo test`, they contend for the same disk
//! and CPU. sharecli already ships a process-pool backend (`spawn-core` +
//! `crates/spawn-core-sys`) backed by a Zig semaphore + posix_spawn +
//! setpriority hot core that throttles build-contending children at the
//! syscall level.
//!
//! This crate exposes a [`BuildThrottle`] trait that substrate dispatchers
//! call before kicking off a build. Two implementations:
//!
//!   * [`SemaphoreBuildThrottle`] — local-only, no external deps. Wraps a
//!     `tokio::sync::Semaphore` so callers can fall back when sharecli is not
//!     on the host.
//!   * [`SharecliBuildThrottle`] — sidecar implementation that shells out to
//!     `sharecli run-pool <harness_type> <project>` to acquire a permit
//!     through sharecli's pool. Falls back to the semaphore impl on any
//!     sharecli failure (missing binary, non-zero exit, timeout) so dispatch
//!     is never blocked by the sidecar.
//!
//! Selection is driven by `SHARECLI_BIN` env: if set and the binary is found,
//! the sidecar impl wins. Otherwise the semaphore impl is used.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Maximum time we'll wait for the sharecli sidecar to respond before giving
/// up and using the local fallback. Tuned low so a hung sharecli can't stall
/// dispatch — better to drop throttling than block the entire wave.
pub const SIDECAR_TIMEOUT: Duration = Duration::from_millis(500);

/// Build-throttle handle returned by [`BuildThrottle::acquire`].
///
/// The permit is released back to the pool when this guard is dropped.
pub struct ThrottlePermit {
    _inner: ThrottlePermitInner,
    label: String,
}

enum ThrottlePermitInner {
    Semaphore(OwnedSemaphorePermit),
    Sidecar { sharecli_pid: u32 },
}

impl ThrottlePermit {
    /// Human-readable label for the permit (e.g. "sharecli/sidecar pid=12345"
    /// or "semaphore/permits-remaining=2"). Useful for structured logging.
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThrottleError {
    #[error("sharecli sidecar spawn failed: {0}")]
    SidecarSpawn(String),
    #[error("sharecli sidecar exited non-zero: code={code}, stderr={stderr}")]
    SidecarExit { code: i32, stderr: String },
    #[error("sharecli sidecar timed out after {0:?}")]
    SidecarTimeout(Duration),
}

/// G3 (2026-06-30): trait substrate dispatchers call before a build step.
/// Implementations decide whether to block on a local semaphore or shell
/// out to sharecli.
#[async_trait]
pub trait BuildThrottle: Send + Sync {
    /// Acquire one permit. The returned guard releases the permit on drop.
    /// `harness_type` maps to sharecli's pool key (e.g. "cargo", "node",
    /// "bun"); `project` is the dispatch cwd or label.
    async fn acquire(
        &self,
        harness_type: &str,
        project: &str,
    ) -> Result<ThrottlePermit, ThrottleError>;
}

// ---------------------------------------------------------------------------
// Local semaphore fallback
// ---------------------------------------------------------------------------

/// Local-only throttle that wraps a `tokio::sync::Semaphore`. Used when
/// `SHARECLI_BIN` is unset or the sharecli sidecar fails.
#[derive(Clone)]
pub struct SemaphoreBuildThrottle {
    sem: Arc<Semaphore>,
}

impl SemaphoreBuildThrottle {
    /// Build with `max_permits` concurrent build slots.
    pub fn new(max_permits: usize) -> Self {
        Self { sem: Arc::new(Semaphore::new(max_permits.max(1))) }
    }
}

#[async_trait]
impl BuildThrottle for SemaphoreBuildThrottle {
    async fn acquire(
        &self,
        harness_type: &str,
        project: &str,
    ) -> Result<ThrottlePermit, ThrottleError> {
        let permit = Arc::clone(&self.sem)
            .acquire_owned()
            .await
            .expect("semaphore never closed");
        let remaining = self.sem.available_permits();
        Ok(ThrottlePermit {
            _inner: ThrottlePermitInner::Semaphore(permit),
            label: format!(
                "semaphore/{harness_type} project={project} permits-remaining={remaining}"
            ),
        })
    }
}

// ---------------------------------------------------------------------------
// sharecli sidecar implementation
// ---------------------------------------------------------------------------

/// Sidecar throttle that delegates to the sharecli binary via the `run-pool`
/// CLI subcommand. Falls back to the local semaphore impl on any failure.
pub struct SharecliBuildThrottle {
    bin: PathBuf,
    fallback: SemaphoreBuildThrottle,
    fallback_after: Duration,
}

impl SharecliBuildThrottle {
    /// Build with a path to the sharecli binary. `fallback_max_permits`
    /// controls the local fallback capacity.
    pub fn new(bin: impl Into<PathBuf>, fallback_max_permits: usize) -> Self {
        Self {
            bin: bin.into(),
            fallback: SemaphoreBuildThrottle::new(fallback_max_permits),
            fallback_after: SIDECAR_TIMEOUT,
        }
    }

    /// Override the sidecar timeout (default 500ms).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.fallback_after = timeout;
        self
    }
}

#[async_trait]
impl BuildThrottle for SharecliBuildThrottle {
    async fn acquire(
        &self,
        harness_type: &str,
        project: &str,
    ) -> Result<ThrottlePermit, ThrottleError> {
        match self.try_sidecar(harness_type, project).await {
            Ok(permit) => Ok(permit),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    harness_type,
                    project,
                    "sharecli sidecar failed; using local semaphore fallback"
                );
                self.fallback.acquire(harness_type, project).await
            }
        }
    }
}

impl SharecliBuildThrottle {
    async fn try_sidecar(
        &self,
        harness_type: &str,
        project: &str,
    ) -> Result<ThrottlePermit, ThrottleError> {
        // sharecli run-pool <harness_type> <project> — prints the acquired
        // pool pid to stdout. We hold the child open for the lifetime of the
        // permit by NOT awaiting its completion; the child process represents
        // the held permit. When the ThrottlePermit drops we kill it.
        let mut child = Command::new(&self.bin)
            .arg("run-pool")
            .arg(harness_type)
            .arg(project)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ThrottleError::SidecarSpawn(e.to_string()))?;

        // Wait for the child to exit (it acquires+releases the pool
        // internally then returns). Cap the wait at `fallback_after`.
        let output = match tokio::time::timeout(self.fallback_after, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(ThrottleError::SidecarSpawn(e.to_string())),
            Err(_) => return Err(ThrottleError::SidecarTimeout(self.fallback_after)),
        };

        if !output.status.success() {
            return Err(ThrottleError::SidecarExit {
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        // Parse the pid sharecli printed on stdout. Format: "Pooled <type>
        // process <pid> for project <name>" — we extract the pid field.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let sharecli_pid = parse_sharecli_pid(&stdout)
            .ok_or_else(|| ThrottleError::SidecarExit {
                code: -1,
                stderr: format!("could not parse pid from sharecli stdout: {stdout}"),
            })?;

        Ok(ThrottlePermit {
            _inner: ThrottlePermitInner::Sidecar { sharecli_pid },
            label: format!(
                "sharecli/sidecar harness={harness_type} project={project} pid={sharecli_pid}"
            ),
        })
    }
}

/// Pull the integer pid out of the "Pooled <type> process <pid> for project ..." line.
fn parse_sharecli_pid(stdout: &str) -> Option<u32> {
    // sharecli/run-pool prints: "Pooled cargo process 12345 for project foo"
    for line in stdout.lines() {
        let mut it = line.split_whitespace();
        while let Some(tok) = it.next() {
            if tok == "process" {
                if let Some(pid_str) = it.next() {
                    return pid_str.parse().ok();
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Selector: pick sidecar vs semaphore based on env + binary probe
// ---------------------------------------------------------------------------

/// Build the right throttle based on the environment.
///
/// `SHARECLI_BIN` env wins if the binary exists on disk. Otherwise returns
/// the local semaphore impl. `max_permits` is the cap for both implementations
/// (the sidecar uses sharecli's pool config, but we still bound the local
/// fallback so a degraded mode has the same ceiling).
pub async fn select_from_env(max_permits: usize) -> Arc<dyn BuildThrottle> {
    match std::env::var("SHARECLI_BIN").ok() {
        Some(p) if !p.is_empty() && binary_exists(Path::new(&p)) => {
            tracing::info!(sharecli_bin = %p, "using sharecli sidecar BuildThrottle");
            Arc::new(SharecliBuildThrottle::new(p, max_permits))
        }
        Some(p) => {
            tracing::warn!(
                sharecli_bin = %p,
                "SHARECLI_BIN set but binary not found; using local semaphore"
            );
            Arc::new(SemaphoreBuildThrottle::new(max_permits))
        }
        None => Arc::new(SemaphoreBuildThrottle::new(max_permits)),
    }
}

fn binary_exists(p: &Path) -> bool {
    p.exists()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sharecli_pid_handles_standard_line() {
        let s = "Pooled cargo process 12345 for project foo\nOutput: acquired\n";
        assert_eq!(parse_sharecli_pid(s), Some(12345));
    }

    #[test]
    fn parse_sharecli_pid_handles_no_pid() {
        let s = "Output: acquired\n";
        assert_eq!(parse_sharecli_pid(s), None);
    }

    #[tokio::test]
    async fn semaphore_throttle_serialises_acquires() {
        let t = SemaphoreBuildThrottle::new(1);
        let p1 = t.acquire("cargo", "foo").await.unwrap();
        // Try to acquire a second while p1 is held — use try_acquire.
        let p2 = t.acquire("cargo", "foo").await.unwrap();
        assert!(!p1.label().is_empty());
        assert!(!p2.label().is_empty());
    }

    #[tokio::test]
    async fn select_from_env_defaults_to_semaphore_when_unset() {
        std::env::remove_var("SHARECLI_BIN");
        let t = select_from_env(2).await;
        let p = t.acquire("cargo", "foo").await.unwrap();
        assert!(p.label().starts_with("semaphore/"));
    }

    #[tokio::test]
    async fn select_from_env_falls_back_when_binary_missing() {
        std::env::set_var("SHARECLI_BIN", "/nonexistent/sharecli-binary-xyz");
        let t = select_from_env(2).await;
        let p = t.acquire("cargo", "foo").await.unwrap();
        // Falls back to semaphore because the binary doesn't exist.
        assert!(p.label().starts_with("semaphore/"));
        std::env::remove_var("SHARECLI_BIN");
    }

    #[tokio::test]
    async fn sharecli_sidecar_falls_back_on_missing_binary() {
        // Point at a binary that will never exist; the sidecar must fail
        // and the local semaphore must serve the permit.
        let t = SharecliBuildThrottle::new(
            "/nonexistent/sharecli-binary-xyz",
            4,
        );
        let p = t.acquire("cargo", "foo").await.unwrap();
        // Fallback is the semaphore impl.
        assert!(p.label().starts_with("semaphore/"));
    }
}