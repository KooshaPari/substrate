use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("arch-test lives under <repo>/crates/arch-test")
        .to_path_buf()
}

#[test]
fn ci_runs_nextest_with_the_ci_profile() {
    let repository_root = repository_root();
    let workflow = std::fs::read_to_string(repository_root.join(".github/workflows/ci.yml"))
        .expect("read CI workflow");
    let nextest = std::fs::read_to_string(repository_root.join(".config/nextest.toml"))
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
        workflow.contains("cargo build -p fake-codex-cloud"),
        "CI must prebuild the cloud-codex conformance fixture before parallel tests start"
    );
    assert!(
        nextest.contains("[profile.ci]"),
        "the repository must define a dedicated ci nextest profile"
    );
    assert!(
        nextest.contains("test-threads = 12"),
        "the ci profile must opt into parallel test execution"
    );
    assert!(
        nextest.contains("[profile.ci.junit]") && nextest.contains("path = \"junit.xml\""),
        "the ci profile must write its JUnit report into nextest's ci store directory"
    );
    assert!(
        workflow.contains("target/nextest/ci/junit.xml"),
        "CI must upload the JUnit report from nextest's profile-specific store directory"
    );
}
