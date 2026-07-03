//! Application state — top-level struct shared by all TUI components.

use std::time::Instant;

use crate::config::TuiConfig;
use crate::proccompose::Composition;

/// Top-level application state.
pub struct App {
    /// Dashboard configuration (gateway URL, compose dir, etc.).
    pub config: TuiConfig,
    /// Whether the gateway is currently reachable.
    pub connected: bool,
    /// Live compositions loaded from the compose directory.
    pub compositions: Vec<Composition>,
    /// A2A tasks tracked by the gateway.
    pub tasks: Vec<Task>,
    /// Instant the dashboard started (for uptime calculation).
    startup: Instant,
}

impl App {
    /// Create a new app state from the given configuration.
    pub fn new(config: TuiConfig) -> Self {
        Self {
            config,
            connected: false,
            compositions: Vec::new(),
            tasks: Vec::new(),
            startup: Instant::now(),
        }
    }

    /// Number of running dispatch lanes across all compositions.
    pub fn active_lanes(&self) -> usize {
        self.compositions
            .iter()
            .flat_map(|c| &c.members)
            .filter(|m| {
                let s = m.state.to_lowercase();
                s == "running" || s == "working"
            })
            .count()
    }

    /// Human-readable uptime since the dashboard started.
    pub fn formatted_uptime(&self) -> String {
        let secs = self.startup.elapsed().as_secs();
        if secs == 0 {
            return "<1s".into();
        }
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;
        if hours > 0 {
            format!("{hours}h {mins}m")
        } else if mins > 0 {
            format!("{mins}m {secs}s")
        } else {
            format!("{secs}s")
        }
    }
}

/// A tracked A2A task.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: String,
}

// ── Pure data transforms (unit-testable, no terminal required) ──────────────

/// Convert raw wire-format A2A task summaries into dashboard [`Task`]s.
///
/// This is extracted as a pure function so the state→render data-transform
/// logic is fully unit-testable without spinning up a gateway or terminal.
pub fn tasks_from_wire(raw: Vec<crate::dispatch_client::A2aTaskSummary>) -> Vec<Task> {
    raw.into_iter()
        .map(|t| {
            let description = t
                .metadata
                .as_ref()
                .and_then(|m| m.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("—")
                .to_owned();
            Task {
                id: t.id.to_string(),
                description,
                status: t.state,
            }
        })
        .collect()
}

/// Summarise a slice of tasks by status for display in the header gauge.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TaskSummary {
    pub total: usize,
    pub working: usize,
    pub completed: usize,
    pub failed: usize,
}

/// Compute a [`TaskSummary`] from a task slice — pure, testable.
pub fn summarise_tasks(tasks: &[Task]) -> TaskSummary {
    let mut s = TaskSummary {
        total: tasks.len(),
        ..Default::default()
    };
    for t in tasks {
        match t.status.to_lowercase().as_str() {
            "working" | "in_progress" | "running" => s.working += 1,
            "completed" | "done" | "succeeded" => s.completed += 1,
            "failed" | "error" | "canceled" => s.failed += 1,
            _ => {}
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch_client::A2aTaskSummary;
    use uuid::Uuid;

    fn wire_task(state: &str) -> A2aTaskSummary {
        A2aTaskSummary {
            id: Uuid::new_v4(),
            state: state.to_owned(),
            team: None,
            assignee: None,
            metadata: None,
        }
    }

    #[test]
    fn tasks_from_wire_maps_state() {
        let raw = vec![
            wire_task("working"),
            wire_task("completed"),
            wire_task("failed"),
        ];
        let tasks = tasks_from_wire(raw);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].status, "working");
        assert_eq!(tasks[1].status, "completed");
        assert_eq!(tasks[2].status, "failed");
    }

    #[test]
    fn tasks_from_wire_empty_metadata_uses_dash() {
        let raw = vec![wire_task("working")];
        let tasks = tasks_from_wire(raw);
        assert_eq!(tasks[0].description, "—");
    }

    #[test]
    fn tasks_from_wire_extracts_description_from_metadata() {
        let mut t = wire_task("working");
        t.metadata = Some(serde_json::json!({ "description": "fix the tests" }));
        let tasks = tasks_from_wire(vec![t]);
        assert_eq!(tasks[0].description, "fix the tests");
    }

    #[test]
    fn tasks_from_wire_empty_input() {
        assert!(tasks_from_wire(vec![]).is_empty());
    }

    #[test]
    fn summarise_tasks_counts_correctly() {
        let tasks = vec![
            Task {
                id: "1".into(),
                description: String::new(),
                status: "working".into(),
            },
            Task {
                id: "2".into(),
                description: String::new(),
                status: "completed".into(),
            },
            Task {
                id: "3".into(),
                description: String::new(),
                status: "failed".into(),
            },
            Task {
                id: "4".into(),
                description: String::new(),
                status: "unknown".into(),
            },
        ];
        let s = summarise_tasks(&tasks);
        assert_eq!(s.total, 4);
        assert_eq!(s.working, 1);
        assert_eq!(s.completed, 1);
        assert_eq!(s.failed, 1);
    }

    #[test]
    fn summarise_tasks_aliases_in_progress() {
        let tasks = vec![
            Task {
                id: "1".into(),
                description: String::new(),
                status: "in_progress".into(),
            },
            Task {
                id: "2".into(),
                description: String::new(),
                status: "running".into(),
            },
            Task {
                id: "3".into(),
                description: String::new(),
                status: "done".into(),
            },
            Task {
                id: "4".into(),
                description: String::new(),
                status: "succeeded".into(),
            },
            Task {
                id: "5".into(),
                description: String::new(),
                status: "error".into(),
            },
            Task {
                id: "6".into(),
                description: String::new(),
                status: "canceled".into(),
            },
        ];
        let s = summarise_tasks(&tasks);
        assert_eq!(s.working, 2);
        assert_eq!(s.completed, 2);
        assert_eq!(s.failed, 2);
    }

    #[test]
    fn summarise_tasks_empty() {
        let s = summarise_tasks(&[]);
        assert_eq!(s, TaskSummary::default());
    }

    #[test]
    fn app_active_lanes_counts_running_members() {
        use crate::proccompose::{Composition, CompositionStatus, Member};
        use std::time::Duration as D;
        use uuid::Uuid;

        let make_member = |state: &str| Member {
            id: Uuid::new_v4(),
            state: state.to_owned(),
            engine: String::new(),
            model: String::new(),
            uptime: D::ZERO,
            prompt_preview: String::new(),
        };

        let comp = Composition {
            name: "test".into(),
            status: CompositionStatus::Running,
            members: vec![
                make_member("running"),
                make_member("working"),
                make_member("stopped"),
                make_member("idle"),
            ],
            uptime: D::ZERO,
        };
        let cfg = crate::config::TuiConfig::default();
        let mut app = App::new(cfg);
        app.compositions = vec![comp];
        assert_eq!(app.active_lanes(), 2);
    }

    #[test]
    fn app_formatted_uptime_zero() {
        let app = App::new(crate::config::TuiConfig::default());
        // Just started — could be <1s or "0s".
        let u = app.formatted_uptime();
        assert!(!u.is_empty());
    }
}
