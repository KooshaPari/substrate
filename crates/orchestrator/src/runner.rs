//! WaveRunner — orchestrates a [`WaveConfig`] against a pluggable dispatcher
//! and emits a [`WaveReport`].
//!
//! The MVP cut-line keeps this single-pass (no auto-retry on transient
//! failures) because retries are already handled by the upstream dispatchers
//! (`forge`, `codex`). We surface failures loud and aggregated.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::dispatcher::Dispatcher;
use crate::error::{OrchestratorError, Result};
use crate::wave::{TaskSpec, WaveConfig};

/// What a single dispatch invocation returned to the runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispatchOutcome {
    /// Whether the dispatch exited cleanly.
    pub success: bool,
    /// Wall-clock duration in milliseconds reported by the dispatcher.
    pub duration_ms: u64,
    /// Cost in USD reported by the dispatcher (zero for stub/mock).
    pub cost_usd: f64,
    /// Optional human-readable summary line for `WaveReport`.
    pub summary: Option<String>,
}

impl DispatchOutcome {
    pub fn success(duration_ms: u64, cost_usd: f64) -> Self {
        Self {
            success: true,
            duration_ms,
            cost_usd,
            summary: None,
        }
    }

    pub fn failure(duration_ms: u64, cost_usd: f64, msg: impl Into<String>) -> Self {
        Self {
            success: false,
            duration_ms,
            cost_usd,
            summary: Some(msg.into()),
        }
    }
}

/// Handle returned to callers for in-flight observation. MVP cut-line only
/// uses this for metric emission; full fan-in is a Phase-2 feature.
#[derive(Debug, Clone)]
pub struct TaskHandle {
    pub alias: String,
    pub started_at: Instant,
}

/// One failed task entry inside a [`WaveReport`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedTask {
    pub alias: String,
    pub module: String,
    pub reason: String,
}

/// Aggregated outcome of a single wave run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaveReport {
    pub name: String,
    pub dispatcher: String,
    pub total: u32,
    pub passed: u32,
    pub failed: Vec<FailedTask>,
    pub total_duration_ms: u64,
    pub total_cost_usd: f64,
}

/// MVP cut-line runner. Fans tasks out concurrently up to
/// `WaveConfig::parallelism`, then assembles a [`WaveReport`].
pub async fn run_wave(config: WaveConfig, dispatcher: Arc<dyn Dispatcher>) -> Result<WaveReport> {
    if config.tasks.is_empty() {
        return Err(OrchestratorError::WaveSchema {
            path: std::path::PathBuf::from("<inline>"),
            message: "run_wave: empty tasks".into(),
        });
    }
    let parallelism = if config.parallelism == 0 {
        config.tasks.len() as u32
    } else {
        config.parallelism
    };

    let started = Instant::now();
    let mut handles = Vec::with_capacity(config.tasks.len());
    let sem = Arc::new(tokio::sync::Semaphore::new(parallelism.max(1) as usize));
    for task in config.tasks.iter().cloned() {
        let permit_src = sem.clone();
        let dispatcher = dispatcher.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit_src.acquire_owned().await.expect("semaphore");
            let start = Instant::now();
            let outcome = match dispatcher.dispatch(&task).await {
                Ok(o) => o,
                Err(e) => {
                    DispatchOutcome::failure(start.elapsed().as_millis() as u64, 0.0, e.to_string())
                }
            };
            (task, outcome)
        }));
    }

    let mut passed: u32 = 0;
    let mut failed: Vec<FailedTask> = Vec::new();
    let mut total_cost: f64 = 0.0;
    for h in handles {
        match h.await {
            Ok((task, outcome)) => {
                total_cost += outcome.cost_usd;
                if outcome.success {
                    passed += 1;
                } else {
                    let reason = outcome
                        .summary
                        .clone()
                        .unwrap_or_else(|| "unknown failure".into());
                    failed.push(FailedTask {
                        alias: task.alias.clone(),
                        module: task.module.clone(),
                        reason,
                    });
                }
            }
            Err(e) => {
                return Err(OrchestratorError::Dispatch {
                    task: "<join>".into(),
                    message: format!("join error: {e}"),
                });
            }
        }
    }

    Ok(WaveReport {
        name: config.name,
        dispatcher: dispatcher.name().to_string(),
        total: config.tasks.len() as u32,
        passed,
        failed,
        total_duration_ms: started.elapsed().as_millis() as u64,
        total_cost_usd: total_cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatcher::MockDispatcher;
    use crate::wave::{DispatcherKind, Expectation, ExpectationKind, TaskSpec};
    use std::sync::Arc;

    fn make_cfg(name: &str, tasks: Vec<TaskSpec>) -> WaveConfig {
        WaveConfig {
            name: name.into(),
            dispatcher: DispatcherKind::Inline,
            tasks,
            parallelism: 2,
            timeout_seconds: 60,
        }
    }

    fn task(alias: &str, module: &str) -> TaskSpec {
        TaskSpec {
            alias: alias.into(),
            module: module.into(),
            prompt_template: format!("ship {alias}"),
            expectations: vec![Expectation {
                kind: ExpectationKind::FileExists,
                value: format!("src/{alias}.rs"),
            }],
            parallelism: 0,
        }
    }

    #[tokio::test]
    async fn runs_two_tasks_with_mock_pass_and_fail() {
        let cfg = make_cfg(
            "mvp-2",
            vec![task("cli", "sharecli::cli"), task("tui", "sharecli::tui")],
        );
        let dispatcher = Arc::new(MockDispatcher::new(
            "mock",
            // Mock pops from the end; queue the outcomes so the FIRST popped
            // is the FIRST dispatched task ("cli" → failure) and the second
            // popped is "tui" → success.
            vec![
                DispatchOutcome::success(0, 0.001),
                DispatchOutcome::failure(0, 0.0, "boom"),
            ],
        ));
        let report = run_wave(cfg, dispatcher.clone()).await.expect("run");
        assert_eq!(report.total, 2);
        assert_eq!(report.passed, 1, "one pass");
        assert_eq!(report.failed.len(), 1, "one fail");
        assert_eq!(report.failed[0].alias, "cli", "fail was first task");
        assert_eq!(report.failed[0].reason, "boom");
        assert!(report.total_cost_usd > 0.0);
        assert_eq!(report.dispatcher, "mock");
        assert_eq!(dispatcher.call_count(), 2);
    }
}
