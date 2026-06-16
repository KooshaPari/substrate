#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Debounced [`WatcherPort`] backed by the `notify` crate.
//!
//! Uses `notify-debouncer-mini` to coalesce rapid filesystem events into a
//! single debounced notification per path. Platform backends (inotify,
//! FSEvents, ReadDirectoryChangesW) are selected by `notify`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::watcher_port::{WatchEvent, WatchEventKind, WatchHandle, WatcherPort};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

fn classify_event(path: &Path, known: &mut HashSet<std::path::PathBuf>) -> WatchEventKind {
    let path_buf = path.to_path_buf();
    if path.exists() {
        if known.insert(path_buf) {
            WatchEventKind::Create
        } else {
            WatchEventKind::Modify
        }
    } else {
        known.remove(path);
        WatchEventKind::Remove
    }
}

fn seed_known_paths(path: &Path, known: &mut HashSet<std::path::PathBuf>) {
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                known.insert(entry.path());
            }
        }
    } else if path.exists() {
        known.insert(path.to_path_buf());
    }
}

struct Subscription {
    rx: Arc<Mutex<mpsc::Receiver<Result<WatchEvent>>>>,
    /// Keeps the debouncer alive for the subscription lifetime.
    _debouncer: Box<dyn std::any::Any + Send>,
}

/// [`WatcherPort`] using `notify` + `notify-debouncer-mini`.
#[derive(Clone)]
pub struct NotifyWatcher {
    subs: Arc<Mutex<HashMap<Uuid, Subscription>>>,
}

impl std::fmt::Debug for NotifyWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyWatcher").finish_non_exhaustive()
    }
}

impl Default for NotifyWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl NotifyWatcher {
    /// Create a new watcher manager.
    pub fn new() -> Self {
        Self {
            subs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl WatcherPort for NotifyWatcher {
    async fn watch(&self, path: &Path, debounce_ms: u64) -> Result<WatchHandle> {
        let (tx, rx) = mpsc::channel(64);
        let debounce = Duration::from_millis(debounce_ms);
        let known_paths = Arc::new(StdMutex::new(HashSet::<std::path::PathBuf>::new()));
        {
            let mut known = known_paths.lock().expect("known_paths lock");
            seed_known_paths(path, &mut known);
        }

        let debouncer = new_debouncer(debounce, {
            let tx = tx.clone();
            let known_paths = Arc::clone(&known_paths);
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    for event in events {
                        let mut known = known_paths.lock().expect("known_paths lock");
                        let kind = classify_event(&event.path, &mut known);
                        let mapped = WatchEvent {
                            path: event.path,
                            kind,
                        };
                        let _ = tx.blocking_send(Ok(mapped));
                    }
                }
                Err(e) => {
                    let _ = tx.blocking_send(Err(SubstrateError::Watcher(format!(
                        "debouncer event: {e}"
                    ))));
                }
            }
        })
        .map_err(|e| SubstrateError::Watcher(format!("debouncer: {e}")))?;

        let mut debouncer = debouncer;
        debouncer
            .watcher()
            .watch(
                path,
                notify_debouncer_mini::notify::RecursiveMode::NonRecursive,
            )
            .map_err(|e| SubstrateError::Watcher(format!("watch {}: {e}", path.display())))?;

        let id = Uuid::new_v4();
        let handle = WatchHandle { id };
        self.subs.lock().await.insert(
            id,
            Subscription {
                rx: Arc::new(Mutex::new(rx)),
                _debouncer: Box::new(debouncer),
            },
        );
        Ok(handle)
    }

    async fn recv_event(
        &self,
        handle: &WatchHandle,
        timeout: Duration,
    ) -> Result<Option<WatchEvent>> {
        let rx = {
            let subs = self.subs.lock().await;
            let sub = subs
                .get(&handle.id)
                .ok_or_else(|| SubstrateError::NotFound(format!("watch {}", handle.id)))?;
            Arc::clone(&sub.rx)
        };

        match tokio::time::timeout(timeout, async { rx.lock().await.recv().await }).await {
            Ok(Some(Ok(event))) => Ok(Some(event)),
            Ok(Some(Err(e))) => Err(e),
            Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn unwatch(&self, handle: &WatchHandle) -> Result<()> {
        self.subs.lock().await.remove(&handle.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    #[tokio::test(flavor = "multi_thread")]
    async fn detects_create_and_modify_in_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let watcher = NotifyWatcher::new();
        let handle = watcher.watch(tmp.path(), 100).await.unwrap();

        let file = tmp.path().join("probe.txt");

        // Create
        fs::write(&file, b"v1").unwrap();
        let mut saw_create = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !saw_create {
            if let Some(event) = watcher
                .recv_event(&handle, Duration::from_millis(500))
                .await
                .unwrap()
            {
                if event.path == file && matches!(event.kind, WatchEventKind::Create) {
                    saw_create = true;
                }
            }
        }

        // Let debouncer settle before the modify probe.
        tokio::time::sleep(Duration::from_millis(300)).await;
        fs::write(&file, b"v2").unwrap();

        let mut saw_modify = false;
        while std::time::Instant::now() < deadline && !saw_modify {
            if let Some(event) = watcher
                .recv_event(&handle, Duration::from_millis(500))
                .await
                .unwrap()
            {
                if event.path == file && event.kind == WatchEventKind::Modify {
                    saw_modify = true;
                }
            }
        }

        watcher.unwatch(&handle).await.unwrap();
        assert!(saw_create, "expected create event for {:?}", file);
        assert!(saw_modify, "expected modify event for {:?}", file);
    }
}
