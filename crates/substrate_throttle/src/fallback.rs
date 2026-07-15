//! Helpers available regardless of which backend is active.

/// Returns true when `sccache` is found somewhere on `$PATH`.
pub fn sccache_on_path() -> bool {
    std::env::var_os("PATH")
        .and_then(|path_var| {
            std::env::split_paths(&path_var).find_map(|dir| {
                let candidate = dir.join("sccache");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            })
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sccache_probe_is_pure() {
        // Should not panic regardless of environment.
        let _ = sccache_on_path();
    }
}