//! `substrate-throttle` — build-contention throttle for substrate dispatch.
//!
//! Crate-level lint: the public API exposes the full sharecli parity surface
//! (try/async acquire, env injection, harness detection). Internal helpers
//! and FFI symbols are not all used in the current cycle; suppress
//! dead_code so the crate compiles clean across the full surface.
//!
#![allow(dead_code)]

//! # Upstream-sync contract
//!
//! # Upstream-sync contract
//!
//! This crate is the **substrate-side mirror** of `KooshaPari/sharecli`'s
//! `spawn_policy` + `spawn-core-sys` (PR #16). The two crates are
//! intentionally 1:1 — every behavioural change in sharecli MUST be
//! backported here, and vice versa. When the two drift, build contention
//! manifests differently between substrate-dispatch consumers and direct
//! sharecli consumers, which defeats the point of cross-repo throttling.
//!
//! Sync ritual (run by the D-domain cycle):
//!   1. `git -C sharecli log --oneline crates/spawn-core/ crates/spawn-core-sys/ src/spawn_policy.rs | head -10`
//!   2. Compare with the same paths here.
//!   3. Port any drift as a commit with `[sharecli-sync]` prefix in the
//!      subject line and a body referencing the sharecli SHA being mirrored.
//!
//! # Architecture (same as sharecli#16)
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │  Rust (orchestration layer)                     │
//! │  config parse · harness detection · env wiring  │
//! │  SpawnPolicy · BuildPermit · is_build_harness   │
//! └──────────────┬───────────────────────────────────┘
//!                │  extern "C"  (C ABI static lib)
//! ┌──────────────▼───────────────────────────────────┐
//! │  Zig (hot core — ../spawn-core/)                │
//! │  spc_semaphore_*  POSIX mutex + condvar         │
//! │  spc_spawn        posix_spawn / fork+exec       │
//! │                   setpriority(PRIO_DARWIN_BG)   │
//! │  spc_waitpid      waitpid(2)                    │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! # Fallback (no Zig)
//!
//! When the Zig static lib cannot be built (zig missing, build error), the
//! Rust types still compile and the policy falls back to a
//! `tokio::sync::Semaphore` keyed on `max_concurrent_builds`. Behaviour is
//! identical at the semaphore level (peak active ≤ max); the only loss is
//! the syscall-level QoS injection (`setpriority`). All public functions
//! remain safe to call regardless of which path is active — the `ZigPath`
//! variant is selected by the build script via a cfg flag.
//!
//! # Wiring (substrate-side)
//!
//! `engine-forge::run_simple` checks `is_build_harness(argv0)` and, when
//! the `substrate_throttle` feature is on, wraps the spawn in
//! `SpawnPolicy::acquire_build_permit().await`. The `FORGE_DAEMON` fast
//! path is preserved — only the gate is added in front.

use std::sync::Arc;

mod config;
mod fallback;
mod harness;

#[cfg(has_zig_spawn_core)]
mod zig;

pub use config::SpawnPolicyConfig;
pub use harness::is_build_harness;

// ---------------------------------------------------------------------------
// SpawnPolicy — public surface
// ---------------------------------------------------------------------------

/// Spawn-policy enforcer. Either backed by the Zig hot core (preferred) or
/// a `tokio::sync::Semaphore` fallback when the Zig static lib is not
/// available at build time. Public API is identical either way.
pub struct SpawnPolicy {
    inner: Inner,
    pub config: SpawnPolicyConfig,
}

enum Inner {
    #[cfg(has_zig_spawn_core)]
    Zig(Arc<zig::ZigSemaphore>),
    #[cfg(feature = "async")]
    Tokio(Arc<tokio::sync::Semaphore>),
    #[cfg(not(feature = "async"))]
    Sync(Arc<std::sync::Mutex<usize>>),
}

/// RAII permit — holds one throttle slot until dropped.
pub struct BuildPermit {
    inner: PermitInner,
    _config: Arc<SpawnPolicyConfig>,
}

enum PermitInner {
    #[cfg(has_zig_spawn_core)]
    Zig(Arc<zig::ZigSemaphore>),
    #[cfg(feature = "async")]
    Tokio(tokio::sync::OwnedSemaphorePermit),
    #[cfg(not(feature = "async"))]
    Sync,
}

impl SpawnPolicy {
    /// Create a new throttle policy with the given config.
    pub fn new(config: SpawnPolicyConfig) -> Self {
        let permits = config.max_concurrent_builds.max(1);
        let inner = Self::build_inner(permits);
        Self { inner, config }
    }

    fn build_inner(permits: usize) -> Inner {
        #[cfg(has_zig_spawn_core)]
        {
            Inner::Zig(Arc::new(zig::ZigSemaphore::new(permits)))
        }
        #[cfg(all(not(has_zig_spawn_core), feature = "async"))]
        {
            Inner::Tokio(Arc::new(tokio::sync::Semaphore::new(permits)))
        }
        #[cfg(all(not(has_zig_spawn_core), not(feature = "async")))]
        {
            // Synchronous fallback — only acquires via try_acquire; used by
            // diagnostic tooling when the async feature is off.
            Inner::Sync(Arc::new(std::sync::Mutex::new(permits)))
        }
    }

    /// Acquire a build permit asynchronously (bridged via `spawn_blocking`
    /// when the Zig path is active, direct `.acquire().await` on tokio).
    #[cfg(feature = "async")]
    pub async fn acquire_build_permit(self: &Arc<Self>) -> anyhow::Result<BuildPermit> {
        let cfg = Arc::new(self.config.clone());
        let inner = match &self.inner {
            #[cfg(has_zig_spawn_core)]
            Inner::Zig(sem) => {
                let sem_for_blocking = Arc::clone(sem);
                tokio::task::spawn_blocking(move || sem_for_blocking.acquire())
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join: {e}"))??;
                PermitInner::Zig(Arc::clone(sem))
            }
            Inner::Tokio(sem) => {
                let p = Arc::clone(sem).acquire_owned().await?;
                PermitInner::Tokio(p)
            }
            #[allow(unreachable_patterns)]
            _ => unreachable!("sync inner cannot reach async API"),
        };
        Ok(BuildPermit { inner, _config: cfg })
    }

    /// Try to acquire without waiting. Returns `None` if at capacity.
    pub fn try_acquire_build_permit(&self) -> Option<BuildPermit> {
        let cfg = Arc::new(self.config.clone());
        let inner = match &self.inner {
            #[cfg(has_zig_spawn_core)]
            Inner::Zig(sem) => {
                if sem.try_acquire().ok()? {
                    PermitInner::Zig(Arc::clone(sem))
                } else {
                    return None;
                }
            }
            #[cfg(feature = "async")]
            Inner::Tokio(sem) => {
                let p = Arc::clone(sem).try_acquire_owned().ok()?;
                PermitInner::Tokio(p)
            }
            #[cfg(not(feature = "async"))]
            Inner::Sync(_) => return Some(BuildPermit { inner: PermitInner::Sync, _config: cfg }),
        };
        Some(BuildPermit { inner, _config: cfg })
    }

    /// Current number of free slots (approximate, advisory only).
    pub fn available_permits(&self) -> usize {
        match &self.inner {
            #[cfg(has_zig_spawn_core)]
            Inner::Zig(sem) => sem.available(),
            #[cfg(feature = "async")]
            Inner::Tokio(sem) => sem.available_permits(),
            #[cfg(not(feature = "async"))]
            Inner::Sync(_) => 0,
        }
    }

    /// Build env-var overrides to inject into a build-harness spawn.
    /// Mirrors sharecli's `SpawnPolicy::build_env_overrides`:
    /// * `CARGO_BUILD_JOBS` — caps rustc's internal parallelism.
    /// * `RUSTC_WRAPPER=sccache` — only when `use_sccache` is on AND
    ///   `sccache` is found on PATH.
    pub fn build_env_overrides(&self) -> Vec<(String, String)> {
        let mut env = vec![(
            "CARGO_BUILD_JOBS".to_string(),
            self.config.max_concurrent_builds.to_string(),
        )];
        if self.config.use_sccache && fallback::sccache_on_path() {
            env.push(("RUSTC_WRAPPER".to_string(), "sccache".to_string()));
        }
        env
    }
}

impl Drop for BuildPermit {
    fn drop(&mut self) {
        match &self.inner {
            #[cfg(has_zig_spawn_core)]
            PermitInner::Zig(sem) => {
                let _ = sem.release();
            }
            // Tokio permit auto-releases on drop; nothing to do.
            #[cfg(feature = "async")]
            PermitInner::Tokio(_) => {}
            #[cfg(not(feature = "async"))]
            PermitInner::Sync => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zig_or_tokio_cap_enforced() {
        let p = SpawnPolicy::new(SpawnPolicyConfig { max_concurrent_builds: 2, ..Default::default() });
        let _a = p.try_acquire_build_permit().expect("first");
        let _b = p.try_acquire_build_permit().expect("second");
        assert!(p.try_acquire_build_permit().is_none(), "must block at cap=2");
        drop(_a);
        assert!(p.try_acquire_build_permit().is_some(), "slot freed after drop");
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn semaphore_queues_excess_tasks() {
        use std::sync::Mutex;
        use tokio::task::JoinSet;

        let policy = Arc::new(SpawnPolicy::new(SpawnPolicyConfig {
            max_concurrent_builds: 2,
            ..Default::default()
        }));
        let active = Arc::new(Mutex::new(0usize));
        let peak = Arc::new(Mutex::new(0usize));

        let mut set = JoinSet::new();
        for _ in 0..6 {
            let policy = Arc::clone(&policy);
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            set.spawn(async move {
                let _permit = policy.acquire_build_permit().await.unwrap();
                {
                    let mut a = active.lock().unwrap();
                    *a += 1;
                    let mut pk = peak.lock().unwrap();
                    if *a > *pk { *pk = *a; }
                }
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                {
                    let mut a = active.lock().unwrap();
                    *a -= 1;
                }
            });
        }
        while set.join_next().await.is_some() {}
        let pk = *peak.lock().unwrap();
        assert!(pk <= 2, "peak active was {pk}, expected ≤ 2");
    }

    #[test]
    fn cargo_build_jobs_injected() {
        let p = SpawnPolicy::new(SpawnPolicyConfig {
            max_concurrent_builds: 3,
            use_sccache: false,
            ..Default::default()
        });
        let env = p.build_env_overrides();
        let jobs = env.iter().find(|(k, _)| k == "CARGO_BUILD_JOBS").map(|(_, v)| v.as_str());
        assert_eq!(jobs, Some("3"));
    }
}