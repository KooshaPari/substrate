//! Bench harness variant of fake-forge. Ignores argv, prints a fixed
//! conversation-id, and exits in <1 ms. Used by `engine-forge`'s F5 bench
//! to measure `forge_daemon_dispatch` overhead — the daemon builds the
//! argv for the child, so the bench needs a binary that doesn't gate on
//! specific flags.
//!
//! Recognised env:
//!   FAKE_FORGE_HANG=1 → sleep forever (same shape as the main fake-forge).

const CONV_ID: &str = "11111111-1111-1111-1111-111111111111";

fn main() {
    println!("conversation-id: {CONV_ID}");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    if std::env::var("FAKE_FORGE_HANG").is_ok() {
        std::thread::sleep(std::time::Duration::from_secs(u64::MAX));
    }
}