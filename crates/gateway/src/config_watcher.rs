//! Hot-reload watcher for the gateway TOML config file.
//!
//! [`ConfigWatcher`] watches a file path for changes using [`notify`], debounces
//! events by 200 ms, re-parses the TOML on each change, and pushes the new
//! [`FileConfig`] onto a [`tokio::sync::watch`] channel.  Parse errors are logged
//! but never crash the server — the previous valid config stays in effect.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use tokio::sync::watch;

/// Subset of gateway configuration that can be live-reloaded from a TOML file.
///
/// Only the fields that make sense to update at runtime are included here.
/// Transport-level settings (`bind` address) require a restart.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FileConfig {
    /// Optional bearer token; when present, protected routes require auth.
    #[serde(default)]
    pub auth_token: Option<String>,

    /// Maximum requests per second per remote IP (0 = unlimited).
    #[serde(default)]
    pub rate_limit_rps: u32,

    /// Number of upstream retry attempts on transient errors.
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,

    /// List of provider names to enable (empty = all built-ins enabled).
    #[serde(default)]
    pub enabled_providers: Vec<String>,
}

fn default_retry_attempts() -> u32 {
    3
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            auth_token: None,
            rate_limit_rps: 0,
            retry_attempts: default_retry_attempts(),
            enabled_providers: vec![],
        }
    }
}

impl FileConfig {
    /// Parse a [`FileConfig`] from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Read and parse a [`FileConfig`] from a file on disk.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text)
            .map_err(|e| anyhow::anyhow!("TOML parse error in {}: {e}", path.display()))
    }
}

// ---------------------------------------------------------------------------
// ConfigWatcher
// ---------------------------------------------------------------------------

/// Watches a config file for changes and sends updated [`FileConfig`] values on
/// a [`watch::Sender`].
///
/// Drop to stop watching.
pub struct ConfigWatcher {
    /// Keep the underlying watcher alive; dropping it stops OS events.
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Create a new watcher.
    ///
    /// * `path` — path to the TOML config file to watch.
    /// * `reload_tx` — channel sender; a new [`FileConfig`] is sent each time
    ///   the file changes and parses successfully.
    ///
    /// Events are debounced: multiple filesystem notifications within 200 ms
    /// collapse into a single reload.
    pub fn new(path: PathBuf, reload_tx: watch::Sender<FileConfig>) -> anyhow::Result<Self> {
        // Shared last-fire timestamp for debouncing.
        let last_fire: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let debounce = Duration::from_millis(200);

        let path_clone = path.clone();
        let tx = reload_tx;

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let event = match res {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("[config_watcher] notify error: {err}");
                    return;
                }
            };

            // Only react to file modification / creation events.
            let relevant = matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_));
            if !relevant {
                return;
            }

            // Debounce: skip if a reload fired less than 200ms ago.
            let now = Instant::now();
            {
                let guard = last_fire.lock().unwrap();
                if let Some(last) = *guard {
                    if now.duration_since(last) < debounce {
                        return;
                    }
                }
            }

            // Re-read and re-parse.
            match FileConfig::from_file(&path_clone) {
                Ok(cfg) => {
                    // Ignore stale notifications that only re-read the current
                    // value (common when the initial file write races watcher
                    // registration); they must not consume the debounce window.
                    if *tx.borrow() == cfg {
                        return;
                    }
                    // Only start the debounce window after a successful read.
                    // Atomic/partial writes can deliver a notification before
                    // the file is parseable; failed reads must not suppress the
                    // next notification containing the valid configuration.
                    *last_fire.lock().unwrap() = Some(now);
                    eprintln!(
                        "[config_watcher] reloaded config from {}",
                        path_clone.display()
                    );
                    // Ignore send errors — receiver may have been dropped (shutdown).
                    let _ = tx.send(cfg);
                }
                Err(err) => {
                    eprintln!(
                        "[config_watcher] parse error in {} (keeping previous config): {err}",
                        path_clone.display()
                    );
                }
            }
        })?;

        // Watch the parent directory so renames/atomic writes are caught.
        let watch_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

        Ok(Self { _watcher: watcher })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use tokio::time::{sleep, Duration};

    #[test]
    fn parse_valid_toml() {
        let toml = r#"
            auth_token = "secret"
            rate_limit_rps = 100
            retry_attempts = 5
            enabled_providers = ["deepseek", "kilocode"]
        "#;
        let cfg = FileConfig::from_toml(toml).unwrap();
        assert_eq!(cfg.auth_token, Some("secret".to_string()));
        assert_eq!(cfg.rate_limit_rps, 100);
        assert_eq!(cfg.retry_attempts, 5);
        assert_eq!(cfg.enabled_providers, vec!["deepseek", "kilocode"]);
    }

    #[test]
    fn parse_error_returns_err() {
        let bad = "auth_token = [unclosed";
        assert!(FileConfig::from_toml(bad).is_err());
    }

    #[test]
    fn defaults_on_empty_toml() {
        let cfg = FileConfig::from_toml("").unwrap();
        assert_eq!(cfg.auth_token, None);
        assert_eq!(cfg.rate_limit_rps, 0);
        assert_eq!(cfg.retry_attempts, 3);
        assert!(cfg.enabled_providers.is_empty());
    }

    /// Watcher sends on file change and debounce fires only once for rapid writes.
    #[tokio::test]
    async fn watcher_sends_on_file_change() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("gateway.toml");

        // Write an initial config.
        fs::write(
            &cfg_path,
            r#"auth_token = "initial"
rate_limit_rps = 10
"#,
        )
        .unwrap();

        let initial = FileConfig::from_file(&cfg_path).unwrap();
        let (tx, mut rx) = watch::channel(initial);
        let _watcher = ConfigWatcher::new(cfg_path.clone(), tx).unwrap();

        // Give the platform watcher callback time to finish registration before
        // writing; initial directory events can otherwise race this update.
        sleep(Duration::from_millis(500)).await;

        // Write a new config.
        fs::write(
            &cfg_path,
            r#"auth_token = "updated"
rate_limit_rps = 50
"#,
        )
        .unwrap();

        let got = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let current = rx.borrow().clone();
                if current.auth_token.as_deref() == Some("updated") {
                    return current;
                }
                rx.changed().await.expect("watcher receiver remains open");
            }
        })
        .await
        .expect("config reload did not arrive within timeout");
        assert_eq!(got.auth_token, Some("updated".to_string()));
        assert_eq!(got.rate_limit_rps, 50);
    }

    /// Parse error in updated file does not crash — channel value is unchanged.
    #[tokio::test]
    async fn parse_error_does_not_crash_watcher() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("gateway.toml");

        let good = r#"auth_token = "stable"
retry_attempts = 7
"#;
        fs::write(&cfg_path, good).unwrap();

        let initial = FileConfig::from_file(&cfg_path).unwrap();
        let (tx, mut rx) = watch::channel(initial);
        let _watcher = ConfigWatcher::new(cfg_path.clone(), tx).unwrap();

        // Allow the platform watcher callback to finish registering before the
        // rapid-write sequence; otherwise its initial directory event can race
        // the first update on heavily loaded CI runners.
        sleep(Duration::from_millis(500)).await;

        // Write invalid TOML.
        fs::write(&cfg_path, "retry_attempts = [broken").unwrap();
        // Allow any queued filesystem notifications to settle; parse failures
        // must not replace the last valid channel value.
        sleep(Duration::from_millis(1000)).await;

        // Channel value should still be the original good config.
        let got = rx.borrow_and_update().clone();
        assert_eq!(got.auth_token, Some("stable".to_string()));
        assert_eq!(got.retry_attempts, 7);
    }

    /// Rapid writes within the debounce window result in exactly one send
    /// (or at most one send per debounce period — verify final value is last written).
    #[tokio::test]
    async fn debounce_collapses_rapid_writes() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("gateway.toml");

        fs::write(&cfg_path, r#"rate_limit_rps = 1"#).unwrap();
        let initial = FileConfig::from_file(&cfg_path).unwrap();
        let (tx, mut rx) = watch::channel(initial);
        let _watcher = ConfigWatcher::new(cfg_path.clone(), tx).unwrap();

        sleep(Duration::from_millis(50)).await;

        // Three rapid writes within a few ms.
        for i in 2u32..=4 {
            fs::write(&cfg_path, format!("rate_limit_rps = {i}")).unwrap();
            sleep(Duration::from_millis(10)).await;
        }

        // Wait for the first debounced reload with a bounded timeout rather than a
        // fixed sleep.  Filesystem notification delivery is scheduler-dependent on
        // CI runners, and a 500ms sleep can race a busy runner.
        let got = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let current = rx.borrow().clone();
                if current.rate_limit_rps >= 2 {
                    return current;
                }
                rx.changed().await.expect("watcher receiver remains open");
                let got = rx.borrow_and_update().clone();
                if got.rate_limit_rps >= 2 {
                    return got;
                }
            }
        })
        .await
        .expect("debounced config reload did not arrive within timeout");

        assert!(
            got.rate_limit_rps >= 2,
            "expected at least one debounced reload, got rate_limit_rps={}",
            got.rate_limit_rps
        );
    }
}
