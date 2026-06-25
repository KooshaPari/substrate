//! Enforces the hexagonal dependency rule: `substrate-core` must not depend on
//! any adapter crate. We parse `substrate-core/Cargo.toml` with the `toml`
//! crate and assert no dependency name matches a forbidden adapter pattern.

use std::path::PathBuf;

/// Prefixes/suffixes that mark an adapter crate; core may never depend on one.
fn is_forbidden(dep: &str) -> bool {
    dep.starts_with("engine-")
        || dep.starts_with("transport-")
        || dep.starts_with("store-")
        || dep.starts_with("driver-")
        || dep.ends_with("-adapter")
}

fn core_manifest_path() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/arch-test
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent() // crates/
        .unwrap()
        .join("substrate-core")
        .join("Cargo.toml")
}

fn collect_dep_names(table: Option<&toml::Table>) -> Vec<String> {
    match table {
        Some(t) => t.keys().cloned().collect(),
        None => Vec::new(),
    }
}

#[test]
fn substrate_core_has_no_adapter_dependencies() {
    let path = core_manifest_path();
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    // `toml` 1.1 dropped support for `text.parse::<toml::Value>()` on a
    // top-level table (returns "unexpected content, expected nothing").
    // `toml::Table` is the correct deserializer for Cargo manifests, which
    // are always a top-level table.
    let manifest: toml::Table = toml::from_str(&text).expect("parse substrate-core Cargo.toml");

    let mut deps = collect_dep_names(manifest.get("dependencies").and_then(|v| v.as_table()));
    deps.extend(collect_dep_names(
        manifest.get("dev-dependencies").and_then(|v| v.as_table()),
    ));
    deps.extend(collect_dep_names(
        manifest
            .get("build-dependencies")
            .and_then(|v| v.as_table()),
    ));

    let offenders: Vec<&String> = deps.iter().filter(|d| is_forbidden(d)).collect();
    assert!(
        offenders.is_empty(),
        "substrate-core must not depend on adapter crates, found: {offenders:?}"
    );
}

#[test]
fn forbidden_predicate_matches_expected_patterns() {
    for d in [
        "engine-forge",
        "transport-file",
        "store-file",
        "driver-cli",
        "foo-adapter",
    ] {
        assert!(is_forbidden(d), "{d} should be forbidden");
    }
    for d in [
        "serde",
        "thiserror",
        "uuid",
        "async-trait",
        "substrate-core",
    ] {
        assert!(!is_forbidden(d), "{d} should be allowed");
    }
}
