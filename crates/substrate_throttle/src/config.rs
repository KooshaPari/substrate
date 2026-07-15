//! Throttle configuration. Mirrors sharecli `SpawnPolicyConfig` 1:1.

#[derive(Debug, Clone)]
pub struct SpawnPolicyConfig {
    /// Maximum number of concurrent build-harness processes.
    pub max_concurrent_builds: usize,
    /// When >0 on macOS, wrap build harnesses in `taskpolicy -b <level>`.
    /// (Zig path uses `setpriority(PRIO_DARWIN_BG)` directly; this is the
    /// portable fallback for the `tokio::Command` path used by sharecli.)
    pub nice_level: u8,
    /// When true, inject `RUSTC_WRAPPER=sccache` if `sccache` is on PATH.
    pub use_sccache: bool,
}

impl Default for SpawnPolicyConfig {
    fn default() -> Self {
        Self {
            // Default cap = 4 — small enough to drop contention on a busy
            // laptop, large enough that headroom for foreground work
            // remains on big builds.
            max_concurrent_builds: 4,
            nice_level: 5,
            use_sccache: true,
        }
    }
}