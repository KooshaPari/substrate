use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("arch-test lives under <repo>/crates/arch-test")
        .to_path_buf()
}

#[test]
fn ci_runs_nextest_with_the_ci_profile_and_junit_report() {
    let repository_root = repository_root();
    let workflow = std::fs::read_to_string(repository_root.join(".github/workflows/ci.yml"))
        .expect("read CI workflow");
    let nextest = std::fs::read_to_string(repository_root.join("nextest.toml"))
        .expect("read nextest configuration");

    assert!(
        workflow.contains("cargo nextest run --workspace --profile ci"),
        "CI must run the workspace test suite through cargo-nextest's ci profile"
    );
    assert!(
        workflow.contains("cargo install --locked cargo-nextest"),
        "CI must install cargo-nextest before running the test suite"
    );
    assert!(
        nextest.contains("[profile.ci.junit]"),
        "the ci profile must emit a JUnit report"
    );
    assert!(
        nextest.contains("path = \"target/nextest/junit.xml\""),
        "the JUnit report path must be stable for CI artifact collection"
    );
}
