//! [`WaveRunner`] — fan out N teammate tasks as parallel lanes.
//!
//! Each lane gets its own [`Supervisor`] plus a separate engine session.
//! Concurrency is bounded by a [`tokio::sync::Semaphore`] with `width` permits.
//! Results are harvested via [`EnginePort::extract_result`] and aggregated into
//! a [`WaveReport`].

use std::sync::Arc;

use psub_a2a::task::Task as A2aTask;
use store_sqlite::SqliteMailboxStore;
use substrate_core::domain::TaskState;
use substrate_core::mailbox_port::{MailboxStore, MailboxTaskState};
use substrate_core::ports::EnginePort;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::error::WaveError;

/// Default maximum sub-task nesting depth (wave=1, teammate=2, subagent=3).
pub const DEFAULT_MAX_DEPTH: usize = 3;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single-lane task specification for the wave runner.
#[derive(Debug, Clone)]
pub struct WaveSpec {
    /// Logical name for this lane. Used as the agent_name and sandbox key.
    pub lane: String,
    /// The initial prompt delivered to the engine for this lane.
    pub prompt: String,
}

impl WaveSpec {
    /// Create a new wave spec.
    pub fn new(lane: impl Into<String>, prompt: impl Into<String>) -> Self {
        WaveSpec {
            lane: lane.into(),
            prompt: prompt.into(),
        }
    }
}

/// Status of a single completed lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaneStatus {
    /// Lane completed successfully.
    Completed,
    /// Lane failed with the given error message.
    Failed(String),
}

/// Per-lane result in the [`WaveReport`].
#[derive(Debug, Clone)]
pub struct LaneResult {
    /// The lane name.
    pub lane: String,
    /// Final status.
    pub status: LaneStatus,
    /// PR URLs discovered in this lane's output.
    pub pr_urls: Vec<String>,
    /// Number of files changed (currently 0; placeholder for future).
    pub files_changed: usize,
}

/// Aggregated report returned by [`WaveRunner::run`].
#[derive(Debug, Clone)]
pub struct WaveReport {
    /// Per-lane results in submission order.
    pub lanes: Vec<LaneResult>,
    /// Number of lanes that completed successfully.
    pub done: usize,
    /// Number of lanes that failed.
    pub failed: usize,
    /// Total lanes.
    pub total: usize,
    /// All PR URLs across all lanes (de-duplicated, order preserved).
    pub pr_urls: Vec<String>,
}

// ---------------------------------------------------------------------------
// WaveRunner
// ---------------------------------------------------------------------------

/// Runs N tasks as parallel lanes, harvests results.
///
/// `E` is any [`EnginePort`] implementation. The store is always
/// [`SqliteMailboxStore`] so the task tree is addressable by all lanes.
pub struct WaveRunner<E>
where
    E: EnginePort + 'static,
{
    engine: Arc<E>,
    store: Arc<SqliteMailboxStore>,
    team_id: String,
    /// Maximum number of lanes running at the same time.
    width: usize,
    /// Maximum task nesting depth (wave root = 1, teammate = 2, subagent = 3, …).
    max_depth: usize,
}

impl<E> WaveRunner<E>
where
    E: EnginePort + 'static,
{
    /// Create a new wave runner.
    ///
    /// * `engine`  — the engine adapter shared across all lanes.
    /// * `store`   — the shared SQLite mailbox + tasklist store.
    /// * `team_id` — team namespace for all tasks created.
    /// * `width`   — semaphore permits (max concurrent lanes).
    pub fn new(
        engine: Arc<E>,
        store: Arc<SqliteMailboxStore>,
        team_id: String,
        width: usize,
    ) -> Self {
        WaveRunner {
            engine,
            store,
            team_id,
            width,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    /// Override the maximum depth (default [`DEFAULT_MAX_DEPTH`]).
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Register a sub-task under `parent_task_id`.
    ///
    /// Fails with [`WaveError::DepthExceeded`] if the resulting depth would
    /// exceed `self.max_depth`.
    pub fn register_subtask(
        &self,
        team_id: &str,
        parent_task_id: Option<Uuid>,
        title: &str,
        owner: &str,
    ) -> Result<A2aTask, WaveError> {
        // Compute the new task's depth.
        let new_depth = if let Some(pid) = parent_task_id {
            task_depth_in_store(&self.store, team_id, pid)
                .map_err(|e| WaveError::DepthCompute(pid, e.to_string()))?
                + 1
        } else {
            1
        };

        if new_depth > self.max_depth {
            return Err(WaveError::DepthExceeded {
                max_depth: self.max_depth,
                actual_depth: new_depth,
                task_id: Uuid::new_v4(),
            });
        }

        let task = A2aTask {
            parent_task_id,
            ..A2aTask::new(team_id, title, owner)
        };
        self.store
            .task_create(&task)
            .map_err(|e| WaveError::Store(e.to_string()))?;
        Ok(task)
    }

    /// Fan out `specs` as parallel lanes and return an aggregated [`WaveReport`].
    ///
    /// One lane failing does NOT abort the wave — it is recorded as
    /// [`LaneStatus::Failed`] in the report while other lanes continue.
    pub async fn run(&self, specs: Vec<WaveSpec>) -> Result<WaveReport, WaveError> {
        let total = specs.len();

        // Create the wave root task in the tasklist.
        let wave_task = {
            let t = A2aTask::new(
                &self.team_id,
                format!(
                    "wave:{}",
                    specs
                        .iter()
                        .map(|s| s.lane.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                "wave-runner",
            );
            self.store
                .task_create(&t)
                .map_err(|e| WaveError::Store(e.to_string()))?;
            t
        };

        let sem = Arc::new(Semaphore::new(self.width));
        let mut handles = Vec::with_capacity(total);

        for spec in specs {
            let engine = Arc::clone(&self.engine);
            let store = Arc::clone(&self.store);
            let team_id = self.team_id.clone();
            let wave_task_id = wave_task.id;
            let sem = Arc::clone(&sem);

            let handle = tokio::spawn(async move {
                // Acquire a concurrency slot.
                let _permit = sem.acquire().await.unwrap();

                // Create a lane task parented under the wave root.
                let lane_task = A2aTask {
                    parent_task_id: Some(wave_task_id),
                    ..A2aTask::new(&team_id, format!("lane:{}", spec.lane), &spec.lane)
                };
                let lane_task_id = lane_task.id;
                if let Err(e) = store.task_create(&lane_task) {
                    return LaneResult {
                        lane: spec.lane.clone(),
                        status: LaneStatus::Failed(format!("store error: {e}")),
                        pr_urls: vec![],
                        files_changed: 0,
                    };
                }

                // Mark as working.
                let _ = store.task_update(lane_task_id, MailboxTaskState::Working, None);

                // Spawn the engine for this lane.
                let core_task = substrate_core::domain::Task::new(&spec.prompt, ".");
                let session_result = engine.start(&core_task).await;

                match session_result {
                    Err(e) => {
                        let _ = store.task_update(
                            lane_task_id,
                            MailboxTaskState::Failed,
                            Some(&e.to_string()),
                        );
                        LaneResult {
                            lane: spec.lane,
                            status: LaneStatus::Failed(e.to_string()),
                            pr_urls: vec![],
                            files_changed: 0,
                        }
                    }
                    Ok(session) => {
                        // Dump the conversation and extract the structured result.
                        let dump_result = engine.dump(&session.conv_id).await;
                        let structured = dump_result
                            .ok()
                            .and_then(|d| engine.extract_result(&d).ok());

                        let (status, pr_urls) = match structured {
                            Some(ref r)
                                if r.status == TaskState::Completed
                                    || r.status == TaskState::Failed =>
                            {
                                let mts = if r.status == TaskState::Completed {
                                    MailboxTaskState::Completed
                                } else {
                                    MailboxTaskState::Failed
                                };
                                let _ = store.task_update(lane_task_id, mts, None);
                                let ls = if r.status == TaskState::Completed {
                                    LaneStatus::Completed
                                } else {
                                    LaneStatus::Failed("engine returned Failed".into())
                                };
                                (ls, r.pr_urls.clone())
                            }
                            Some(ref r) => {
                                let _ = store.task_update(
                                    lane_task_id,
                                    MailboxTaskState::Completed,
                                    None,
                                );
                                (LaneStatus::Completed, r.pr_urls.clone())
                            }
                            None => {
                                // Session started but dump/extract gave nothing — treat as completed
                                // with no PR urls (the session text may be in the logfile).
                                let pr_urls = session
                                    .logfile
                                    .as_deref()
                                    .map(harvest_pr_urls)
                                    .unwrap_or_default();
                                let _ = store.task_update(
                                    lane_task_id,
                                    MailboxTaskState::Completed,
                                    None,
                                );
                                (LaneStatus::Completed, pr_urls)
                            }
                        };

                        LaneResult {
                            lane: spec.lane,
                            status,
                            pr_urls,
                            files_changed: 0,
                        }
                    }
                }
                // _permit dropped here, freeing the concurrency slot.
            });
            handles.push(handle);
        }

        // Harvest all lanes.
        let mut lanes: Vec<LaneResult> = Vec::with_capacity(total);
        for h in handles {
            // JoinError (panic) counts as a failed lane.
            match h.await {
                Ok(result) => lanes.push(result),
                Err(e) => lanes.push(LaneResult {
                    lane: "<panic>".into(),
                    status: LaneStatus::Failed(format!("task panicked: {e}")),
                    pr_urls: vec![],
                    files_changed: 0,
                }),
            }
        }

        // Aggregate.
        let done = lanes
            .iter()
            .filter(|r| r.status == LaneStatus::Completed)
            .count();
        let failed = lanes
            .iter()
            .filter(|r| !matches!(r.status, LaneStatus::Completed))
            .count();

        // De-duplicate PR urls preserving order.
        let mut seen = std::collections::HashSet::new();
        let pr_urls: Vec<String> = lanes
            .iter()
            .flat_map(|r| r.pr_urls.iter().cloned())
            .filter(|url| seen.insert(url.clone()))
            .collect();

        Ok(WaveReport {
            lanes,
            done,
            failed,
            total,
            pr_urls,
        })
    }
}

// ---------------------------------------------------------------------------
// Public helper: task depth computation
// ---------------------------------------------------------------------------

/// Compute the depth of a task in the store (root = 1).
///
/// Walks up `parent_task_id` links, counting hops. Returns an error if a
/// cycle is detected (depth > 64) or a referenced parent is missing.
pub fn task_depth_in_store(
    store: &SqliteMailboxStore,
    team_id: &str,
    task_id: Uuid,
) -> Result<usize, WaveError> {
    let tasks = store
        .task_list(team_id)
        .map_err(|e| WaveError::Store(e.to_string()))?;

    // Build a map from id → parent_id.
    let parent_map: std::collections::HashMap<Uuid, Option<Uuid>> =
        tasks.iter().map(|t| (t.id, t.parent_task_id)).collect();

    let mut depth = 1usize;
    let mut current = task_id;
    const MAX_WALK: usize = 64;

    loop {
        match parent_map.get(&current) {
            None => {
                return Err(WaveError::DepthCompute(
                    task_id,
                    format!("task {current} not found in team {team_id}"),
                ))
            }
            Some(None) => break, // root
            Some(Some(pid)) => {
                depth += 1;
                if depth > MAX_WALK {
                    return Err(WaveError::DepthCompute(
                        task_id,
                        "depth walk exceeded 64 — possible cycle".into(),
                    ));
                }
                current = *pid;
            }
        }
    }
    Ok(depth)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract GitHub PR URLs from free text using the canonical PR regex.
fn harvest_pr_urls(text: &str) -> Vec<String> {
    use regex::Regex;
    // Reuse the same pattern as engine-forge parse.rs.
    let re = Regex::new(r"https://github\.com/[^\s]+/pull/\d+").unwrap();
    re.find_iter(text).map(|m| m.as_str().to_string()).collect()
}
