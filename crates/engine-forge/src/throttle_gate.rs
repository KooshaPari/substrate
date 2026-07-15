//! G3 (2026-07-01): build-contention throttle gate for `engine-forge::run_simple`.
//!
//! When the `substrate_throttle` feature is on AND `SUBSTRATE_THROTTLE=1` is
//! set AND `argv[0]` (or the forge binary basename) matches a build harness,
//! the gate acquires a `SpawnPolicy` permit before spawning and holds it for
//! the duration of the child. Non-build harnesses (`forge`, `python`, ...)
//! pass through unchanged.
//!
//! Mirrors `KooshaPari/sharecli#16` ProcessPool behaviour for the substrate
//! dispatch path. The `is_build_harness` predicate lives in
//! `substrate_throttle::harness` so the upstream-sync contract holds:
//! sharecli's heuristic is the source of truth, substrate_throttle mirrors it.
//!
//! # Why opt-in (env + feature)
//!
//! Throttling changes scheduling semantics. Existing engine-forge callers
//! (drivers, MCP server, gateway) must NOT see behaviour change. By gating
//! on `substrate_throttle` Cargo feature AND `SUBSTRATE_THROTTLE=1`, the
//! gate is dormant unless a deployer explicitly opts in. Once enabled, the
//! gate is observation-only w.r.t. correctness — it can only delay a
//! spawn, never change its outcome.

use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

use substrate_throttle::{is_build_harness, SpawnPolicy, SpawnPolicyConfig};

/// Process-wide `SpawnPolicy` Arc. Lazily constructed on first gate use so
/// the crate has no startup cost when the feature/env are off.
static POLICY: OnceLock<Arc<SpawnPolicy>> = OnceLock::new();

fn policy() -> Arc<SpawnPolicy> {
    POLICY
        .get_or_init(|| {
            Arc::new(SpawnPolicy::new(SpawnPolicyConfig {
                max_concurrent_builds: std::env::var("SUBSTRATE_THROTTLE_MAX")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(4),
                nice_level: 5,
                use_sccache: std::env::var("SUBSTRATE_THROTTLE_SCCACHE").ok().as_deref()
                    == Some("1"),
            }))
        })
        .clone()
}

/// True when the gate should fire for this spawn.
pub fn should_throttle(argv0_or_bin: &str) -> bool {
    if std::env::var("SUBSTRATE_THROTTLE").ok().as_deref() != Some("1") {
        return false;
    }
    // argv0 may be a full path — check basename + the bare name.
    let basename = Path::new(argv0_or_bin)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(argv0_or_bin);
    is_build_harness(basename) || is_build_harness(argv0_or_bin)
}

/// Acquire a build permit if the gate is on for this argv. Returns
/// `Some(permit)` when the gate fires, `None` otherwise. The permit MUST
/// be held until the child process completes (or fails to spawn).
pub async fn acquire(argv0_or_bin: &str) -> Option<substrate_throttle::BuildPermit> {
    if !should_throttle(argv0_or_bin) {
        return None;
    }
    match policy().acquire_build_permit().await {
        Ok(permit) => Some(permit),
        Err(e) => {
            eprintln!("[engine-forge] substrate_throttle acquire failed: {e}; passing through");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_off_when_env_missing() {
        std::env::remove_var("SUBSTRATE_THROTTLE");
        assert!(!should_throttle("cargo"));
        assert!(!should_throttle("/usr/bin/cargo"));
        assert!(!should_throttle("rustc"));
    }

    #[test]
    fn gate_off_for_non_build_harness_even_with_env() {
        std::env::set_var("SUBSTRATE_THROTTLE", "1");
        assert!(!should_throttle("forge"));
        assert!(!should_throttle("python"));
        std::env::remove_var("SUBSTRATE_THROTTLE");
    }

    #[test]
    fn gate_on_for_build_harness_with_env() {
        std::env::set_var("SUBSTRATE_THROTTLE", "1");
        assert!(should_throttle("cargo"));
        assert!(should_throttle("/usr/local/bin/cargo"));
        assert!(should_throttle("rustc"));
        assert!(should_throttle("make"));
        assert!(should_throttle("ninja"));
        std::env::remove_var("SUBSTRATE_THROTTLE");
    }

    #[tokio::test]
    async fn acquire_returns_none_when_gate_off() {
        std::env::remove_var("SUBSTRATE_THROTTLE");
        assert!(acquire("cargo").await.is_none());
    }

    #[tokio::test]
    async fn acquire_returns_none_for_non_build_harness() {
        std::env::set_var("SUBSTRATE_THROTTLE", "1");
        assert!(acquire("forge").await.is_none());
        std::env::remove_var("SUBSTRATE_THROTTLE");
    }
}