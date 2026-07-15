//! Build-harness classification. 1:1 with sharecli `is_build_harness`.

/// Returns `true` for harnesses that consume heavy CPU and benefit from
/// throttling. Mirrors sharecli PR #16 — keep in sync.
pub fn is_build_harness(harness: &str) -> bool {
    matches!(
        harness,
        "cargo" | "rustc" | "build" | "make" | "cmake" | "ninja" | "bazel"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_harness_detection() {
        assert!(is_build_harness("cargo"));
        assert!(is_build_harness("rustc"));
        assert!(is_build_harness("make"));
        assert!(is_build_harness("cmake"));
        assert!(is_build_harness("ninja"));
        assert!(is_build_harness("bazel"));
        assert!(is_build_harness("build"));
        assert!(!is_build_harness("forge"));
        assert!(!is_build_harness("python"));
        assert!(!is_build_harness(""));
    }
}