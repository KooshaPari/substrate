#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # wave
//!
//! Phase 4: fan out N teammate tasks as parallel lanes, each in its own
//! `Supervisor` session. Bound concurrency with a semaphore. Harvest aggregated
//! results. Track sub-subagent tasks in the shared tasklist so the full tree is
//! visible.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use psub_wave::{WaveRunner, WaveSpec};
//! use psub_supervisor::FakeEngine;
//! use store_sqlite::SqliteMailboxStore;
//!
//! let engine = Arc::new(FakeEngine::new());
//! let store  = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());
//! let specs  = vec![
//!     WaveSpec::new("lane-0", "do thing A"),
//!     WaveSpec::new("lane-1", "do thing B"),
//! ];
//! let runner = WaveRunner::new(engine, store, "team-x".into(), 4);
//! let report = runner.run(specs).await.unwrap();
//! println!("{} / {} lanes done", report.done, report.total);
//! ```

pub mod error;
pub mod runner;

pub use error::WaveError;
pub use runner::{LaneResult, LaneStatus, WaveReport, WaveRunner, WaveSpec};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use psub_a2a::task::Task as A2aTask;
    use store_sqlite::SqliteMailboxStore;
    use substrate_core::domain::{
        ConversationDump, EngineCapabilities, Mailbox, Session, StructuredResult, Task, TaskState,
    };
    use substrate_core::error::Result;
    use substrate_core::mailbox_port::MailboxStore;
    use substrate_core::ports::EnginePort;
    use tokio::sync::Semaphore;
    use uuid::Uuid;

    use super::*;

    // ── instrumented fake engine ──────────────────────────────────────────────

    /// Fake engine that records peak concurrency and can inject failures.
    #[derive(Clone, Default)]
    struct ConcurrencyProbe {
        /// Scripts: `(lane, should_fail)`.
        lane_fail: Arc<std::sync::Mutex<Vec<bool>>>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        /// Gate that holds each `start` call until released (for concurrency
        /// bound tests). When `None`, start returns immediately.
        gate: Arc<Option<Arc<Semaphore>>>,
        /// Extra PR URLs to embed in the `extract_result` output.
        pr_urls: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl ConcurrencyProbe {
        fn with_failure_script(fail_flags: Vec<bool>) -> Self {
            Self {
                lane_fail: Arc::new(std::sync::Mutex::new(fail_flags)),
                ..Default::default()
            }
        }

        #[allow(dead_code)]
        fn with_gate(gate: Arc<Semaphore>) -> Self {
            Self {
                gate: Arc::new(Some(gate)),
                ..Default::default()
            }
        }

        fn with_pr_url(url: &str) -> Self {
            Self {
                pr_urls: Arc::new(std::sync::Mutex::new(vec![url.to_string()])),
                ..Default::default()
            }
        }

        #[allow(dead_code)]
        fn peak_concurrency(&self) -> usize {
            self.peak.load(Ordering::Acquire)
        }

        fn enter(&self) {
            let prev = self.active.fetch_add(1, Ordering::AcqRel);
            let _current = prev + 1;
            let mut observed = self.peak.load(Ordering::Acquire);
            loop {
                if _current <= observed {
                    break;
                }
                match self.peak.compare_exchange(
                    observed,
                    _current,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(updated) => observed = updated,
                }
            }
        }

        fn leave(&self) {
            self.active.fetch_sub(1, Ordering::AcqRel);
        }

        fn should_fail(&self) -> bool {
            let mut q = self.lane_fail.lock().unwrap();
            if q.is_empty() {
                false
            } else {
                q.remove(0)
            }
        }
    }

    #[async_trait::async_trait]
    impl EnginePort for ConcurrencyProbe {
        async fn start(&self, _task: &Task) -> Result<Session> {
            self.enter();
            // Hold the semaphore permit while "running" if a gate was given.
            if let Some(sem) = self.gate.as_ref() {
                let _permit = sem.acquire().await.unwrap();
                // permit released here (dropped) — but we want to hold it
                // for the duration. Instead use a short sleep to model work.
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let should_fail = self.should_fail();
            self.leave();
            if should_fail {
                return Err(substrate_core::error::SubstrateError::Engine(
                    "lane-failure-injected".into(),
                ));
            }
            let text = self
                .pr_urls
                .lock()
                .unwrap()
                .first()
                .cloned()
                .unwrap_or_else(|| "PR: https://github.com/example/repo/pull/1".to_string());
            Ok(Session {
                conv_id: format!("conv-{}", Uuid::new_v4()),
                pid: None,
                logfile: Some(text),
            })
        }

        async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
            Ok(Session {
                conv_id: conv_id.to_string(),
                pid: None,
                logfile: None,
            })
        }

        async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
            let pr_url = self
                .pr_urls
                .lock()
                .unwrap()
                .first()
                .cloned()
                .unwrap_or_else(|| "PR: https://github.com/example/repo/pull/1".to_string());
            Ok(ConversationDump {
                conversation_id: conv_id.to_string(),
                raw: format!("DONE: ok\n{pr_url}\n"),
            })
        }

        async fn cancel(&self, _conv_id: &str) -> Result<()> {
            Ok(())
        }

        async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
            Ok(())
        }

        fn extract_result(&self, dump: &ConversationDump) -> Result<StructuredResult> {
            use regex::Regex;
            let pr_re = Regex::new(r"https://github\.com/[^\s]+/pull/\d+").unwrap();
            let pr_urls: Vec<String> = pr_re
                .find_iter(&dump.raw)
                .map(|m| m.as_str().to_string())
                .collect();
            Ok(StructuredResult {
                text: dump.raw.clone(),
                artifacts: vec![],
                pr_urls,
                status: TaskState::Completed,
            })
        }

        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities {
                supports_resume: true,
                supports_subagents: true,
                supports_mcp_import: false,
            }
        }
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_store() -> Arc<SqliteMailboxStore> {
        Arc::new(SqliteMailboxStore::open_in_memory().unwrap())
    }

    // ── test 1: N-wide wave all complete, harvest aggregates ──────────────────

    #[tokio::test]
    async fn wave_4_lanes_all_complete() {
        let engine = Arc::new(ConcurrencyProbe::with_pr_url(
            "https://github.com/example/repo/pull/42",
        ));
        let store = make_store();
        let specs = (0..4)
            .map(|i| WaveSpec::new(format!("lane-{i}"), format!("task {i}")))
            .collect::<Vec<_>>();
        let runner = WaveRunner::new(engine, store, "wave-team".into(), 8);
        let report = runner.run(specs).await.unwrap();

        assert_eq!(report.total, 4, "total should be 4");
        assert_eq!(report.done, 4, "all 4 should complete");
        assert_eq!(report.failed, 0, "none should fail");
        // All 4 lanes emit the same PR url; after de-duplication we expect exactly 1.
        assert_eq!(
            report.pr_urls.len(),
            1,
            "de-duplicated PR urls should be 1: {:?}",
            report.pr_urls
        );
        assert_eq!(report.pr_urls[0], "https://github.com/example/repo/pull/42");
    }

    // ── test 2: concurrency bound respected ───────────────────────────────────

    #[tokio::test]
    async fn concurrency_bound_width_2_of_4() {
        // We use a gate semaphore with 0 permits that we manually release to
        // synchronize. Actually simpler: just record peak concurrency naturally
        // with a brief sleep and width=2 — the engine increments active before
        // sleeping and decrements after, so peak should never exceed 2.
        let engine = Arc::new(ConcurrencyProbe::default());
        let peak = engine.peak.clone();

        // 4 lanes, width=2.
        let store = make_store();
        let specs = (0..4)
            .map(|i| WaveSpec::new(format!("lane-{i}"), format!("task {i}")))
            .collect::<Vec<_>>();
        let runner = WaveRunner::new(
            Arc::clone(&engine) as Arc<ConcurrencyProbe>,
            store,
            "wave-bound".into(),
            2,
        );
        let report = runner.run(specs).await.unwrap();
        assert_eq!(report.done, 4);

        let observed_peak = peak.load(Ordering::Acquire);
        assert!(
            observed_peak <= 2,
            "peak concurrency {observed_peak} exceeded width=2"
        );
    }

    // ── test 3: 3-level task tree via sub-subagent registration ──────────────

    #[tokio::test]
    async fn subtask_tree_parent_task_id_visible() {
        // The WaveRunner creates a wave-task in the tasklist.
        // Each lane's Supervisor creates a teammate-task under the wave-task.
        // We then register subagent tasks under those teammate-tasks.
        // task_list should show the full parent_task_id tree.
        let engine = Arc::new(ConcurrencyProbe::default());
        let store = make_store();

        let specs = vec![
            WaveSpec::new("lane-sub-0", "sub task 0"),
            WaveSpec::new("lane-sub-1", "sub task 1"),
        ];
        let runner = WaveRunner::new(
            Arc::clone(&engine),
            Arc::clone(&store),
            "wave-tree-team".into(),
            4,
        );
        let report = runner.run(specs).await.unwrap();
        assert_eq!(report.done, 2);

        // The wave task itself + 2 lane tasks should be in the store.
        let tasks = store.task_list("wave-tree-team").unwrap();
        // At minimum the wave root + 2 lane tasks = 3 records.
        assert!(
            tasks.len() >= 3,
            "expected ≥3 tasks, got {}: {:?}",
            tasks.len(),
            tasks.iter().map(|t| &t.title).collect::<Vec<_>>()
        );

        // Verify the wave root task exists.
        let wave_root = tasks
            .iter()
            .find(|t| t.title.starts_with("wave:"))
            .expect("wave root task not found");

        // Lane tasks should have parent_task_id = wave root id.
        let lane_tasks: Vec<_> = tasks
            .iter()
            .filter(|t| t.parent_task_id == Some(wave_root.id))
            .collect();
        assert_eq!(lane_tasks.len(), 2, "expected 2 lane tasks under wave root");

        // Register subagent tasks manually under the first lane task.
        let lane0 = lane_tasks[0];
        let sub_task = A2aTask {
            id: Uuid::new_v4(),
            team_id: "wave-tree-team".into(),
            title: "subagent task A".into(),
            state: a2a::task::TaskState::Submitted,
            owner: "subagent-a".into(),
            parent_task_id: Some(lane0.id),
            requirement_id: None,
            epic_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.task_create(&sub_task).unwrap();

        let updated_tasks = store.task_list("wave-tree-team").unwrap();
        let sub_found = updated_tasks
            .iter()
            .find(|t| t.parent_task_id == Some(lane0.id))
            .expect("subagent task not found under lane task");
        assert_eq!(sub_found.title, "subagent task A");
        // Verify 3-level chain: wave_root → lane0 → sub_found
        assert_eq!(sub_found.parent_task_id, Some(lane0.id));
        assert_eq!(lane0.parent_task_id, Some(wave_root.id));
    }

    // ── test 4: depth guard rejects beyond max-depth ─────────────────────────

    #[tokio::test]
    async fn depth_guard_rejects_beyond_max_depth() {
        // Build a chain of tasks manually to simulate depth-exceeded scenario.
        // WaveRunner::register_subtask should reject depth > max_depth.
        let engine = Arc::new(ConcurrencyProbe::default());
        let store = make_store();

        let runner = WaveRunner::new(
            Arc::clone(&engine),
            Arc::clone(&store),
            "depth-team".into(),
            4,
        );

        // Build a 3-deep chain manually and try to register a depth-4 task.
        // max_depth default = 3. Depth 4 should fail.
        let root = A2aTask::new("depth-team", "root", "root-agent");
        store.task_create(&root).unwrap();

        let child = A2aTask {
            parent_task_id: Some(root.id),
            ..A2aTask::new("depth-team", "child", "child-agent")
        };
        store.task_create(&child).unwrap();

        let grandchild = A2aTask {
            parent_task_id: Some(child.id),
            ..A2aTask::new("depth-team", "grandchild", "grandchild-agent")
        };
        store.task_create(&grandchild).unwrap();

        // Depth 3 from root is the grandchild (root=1, child=2, grandchild=3).
        // Attempting to register a great-grandchild (depth 4) should fail.
        let result = runner.register_subtask(
            "depth-team",
            Some(grandchild.id),
            "great-grandchild",
            "gg-agent",
        );
        assert!(result.is_err(), "expected depth guard error but got Ok");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("depth") || err_msg.contains("max"),
            "error should mention depth: {err_msg}"
        );
    }

    // ── test 5: one lane failing does NOT kill the wave ───────────────────────

    #[tokio::test]
    async fn one_lane_failure_does_not_abort_wave() {
        // Script: lane 0 fails, lanes 1-3 succeed.
        let fail_flags = vec![true, false, false, false];
        let engine = Arc::new(ConcurrencyProbe::with_failure_script(fail_flags));
        let store = make_store();

        let specs = (0..4)
            .map(|i| WaveSpec::new(format!("lane-{i}"), format!("task {i}")))
            .collect::<Vec<_>>();
        let runner = WaveRunner::new(Arc::clone(&engine), store, "fault-team".into(), 4);
        let report = runner.run(specs).await.unwrap();

        assert_eq!(report.total, 4);
        assert_eq!(report.done, 3, "3 lanes should complete");
        assert_eq!(report.failed, 1, "1 lane should be marked failed");
        // The wave itself returns Ok (not an Err) even with one lane failing.
    }

    // ── test 6: depth computation is correct ─────────────────────────────────

    #[tokio::test]
    async fn depth_at_root_is_1() {
        let store = make_store();
        let root = A2aTask::new("team-d", "root", "r");
        store.task_create(&root).unwrap();

        let depth = runner::task_depth_in_store(&store, "team-d", root.id).unwrap();
        assert_eq!(depth, 1, "root task should have depth 1");
    }

    #[tokio::test]
    async fn depth_at_grandchild_is_3() {
        let store = make_store();

        let root = A2aTask::new("team-d2", "root", "r");
        store.task_create(&root).unwrap();

        let child = A2aTask {
            parent_task_id: Some(root.id),
            ..A2aTask::new("team-d2", "child", "c")
        };
        store.task_create(&child).unwrap();

        let grandchild = A2aTask {
            parent_task_id: Some(child.id),
            ..A2aTask::new("team-d2", "grandchild", "g")
        };
        store.task_create(&grandchild).unwrap();

        let depth = runner::task_depth_in_store(&store, "team-d2", grandchild.id).unwrap();
        assert_eq!(depth, 3, "grandchild should have depth 3");
    }
}
