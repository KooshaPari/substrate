//! The `Supervisor<E, S>` — manages one teammate lane end-to-end.
//!
//! Lifecycle:
//! 1. [`Supervisor::spawn`] — start the engine, record `conv_id`, wire mailbox,
//!    create an A2A task record.
//! 2. [`Supervisor::pump_one`] — claim one inbox message, resume the engine,
//!    mark consumed; Question kind → task transitions to `InputRequired`.
//! 3. [`Supervisor::pump_loop`] — iterate `pump_one` up to `max_turns`, stop on
//!    [`SupervisorError::NoMessages`].

use std::sync::Arc;

use psub_a2a::message::{Message, MessageKind, Part};
use psub_a2a::task::Task as A2aTask;
use psub_a2a::task::TaskState;
use substrate_core::domain::{Mailbox, Task};
use substrate_core::mailbox_port::{MailboxStore, MailboxTaskState};
use substrate_core::ports::EnginePort;

use crate::error::SupervisorError;

/// Configuration for one supervisor lane.
#[derive(Debug, Clone)]
pub struct LaneConfig {
    /// The team identifier used when posting/reading mailbox messages.
    pub team_id: String,
    /// The logical name of the supervised agent (mailbox address).
    pub agent_name: String,
}

impl LaneConfig {
    /// Create a new lane config.
    pub fn new(team_id: impl Into<String>, agent_name: impl Into<String>) -> Self {
        LaneConfig {
            team_id: team_id.into(),
            agent_name: agent_name.into(),
        }
    }
}

/// Manages one teammate lane: spawn → pump → restart → resume-400 fallback.
///
/// `E` is the engine adapter; `S` is the mailbox+task store.
pub struct Supervisor<E, S>
where
    E: EnginePort,
    S: MailboxStore<Msg = Message, Task = A2aTask>,
    S::Error: std::fmt::Display,
{
    engine: Arc<E>,
    store: Arc<S>,
    config: LaneConfig,
    /// Engine conversation id, set after [`spawn`](Supervisor::spawn).
    conv_id: Option<String>,
    /// The A2A task id created at spawn time.
    task_id: Option<uuid::Uuid>,
}

impl<E, S> Supervisor<E, S>
where
    E: EnginePort,
    S: MailboxStore<Msg = Message, Task = A2aTask>,
    S::Error: std::fmt::Display,
{
    /// Create a new supervisor backed by the given engine and store.
    pub fn new(engine: Arc<E>, store: Arc<S>, config: LaneConfig) -> Self {
        Supervisor {
            engine,
            store,
            config,
            conv_id: None,
            task_id: None,
        }
    }

    /// The engine conversation id (set after [`spawn`]).
    pub fn conv_id(&self) -> Option<&str> {
        self.conv_id.as_deref()
    }

    /// Start the engine with `prompt0`, wire the mailbox, and create an A2A task record.
    pub async fn spawn(&mut self, prompt0: &str) -> Result<(), SupervisorError> {
        // Start the engine.
        let core_task = Task::new(prompt0, ".");
        let session = self
            .engine
            .start(&core_task)
            .await
            .map_err(|e| SupervisorError::Engine(e.to_string()))?;

        let conv_id = session.conv_id.clone();

        // Wire mailbox (best-effort; we wrap as MailboxWire if it fails).
        let mailbox = Mailbox {
            owner: self.config.agent_name.clone(),
            messages: vec![],
        };
        self.engine
            .wire_mailbox(&conv_id, &mailbox)
            .await
            .map_err(|e| SupervisorError::MailboxWire(e.to_string()))?;

        // Create A2A task record.
        let task = A2aTask::new(
            self.config.team_id.clone(),
            format!("spawn:{conv_id}"),
            self.config.agent_name.clone(),
        );
        let task_id = task.id;
        self.store
            .task_create(&task)
            .map_err(|e| SupervisorError::Store(e.to_string()))?;
        self.store
            .task_update(
                task_id,
                MailboxTaskState::Working,
                Some("supervisor spawned"),
            )
            .map_err(|e| SupervisorError::Store(e.to_string()))?;

        self.conv_id = Some(conv_id);
        self.task_id = Some(task_id);
        Ok(())
    }

    /// Recover the newest active task for this lane from the durable task list.
    ///
    /// Tasks created by [`spawn`](Supervisor::spawn) use `spawn:<conv_id>` as
    /// their title. Recovery scans persisted tasks for this team/agent, selects
    /// the newest Submitted/Working/InputRequired task, and restores the
    /// supervisor's in-memory `conv_id` and `task_id`.
    pub fn recover_active(&mut self) -> Result<bool, SupervisorError> {
        let tasks = self
            .store
            .task_list(&self.config.team_id)
            .map_err(|e| SupervisorError::Store(e.to_string()))?;

        let Some(task) = tasks
            .into_iter()
            .filter(|task| {
                task.owner == self.config.agent_name
                    && task.title.starts_with("spawn:")
                    && matches!(
                        task.state,
                        TaskState::Submitted | TaskState::Working | TaskState::InputRequired
                    )
            })
            .max_by_key(|task| task.updated_at)
        else {
            return Ok(false);
        };

        let conv_id = task.title.trim_start_matches("spawn:").to_string();
        self.conv_id = Some(conv_id);
        self.task_id = Some(task.id);
        Ok(true)
    }

    /// Claim one unread inbox message, resume the engine, mark consumed.
    ///
    /// For [`MessageKind::Question`] messages the task is transitioned to
    /// `InputRequired`. For a resume-400 engine error the call retries once
    /// with a `"[context stripped]\n"` prefix.
    pub async fn pump_one(&mut self) -> Result<(), SupervisorError> {
        let conv_id = self
            .conv_id
            .as_deref()
            .ok_or_else(|| SupervisorError::Engine("not spawned yet".into()))?;

        // Fetch inbox.
        let msgs = self
            .store
            .inbox(&self.config.team_id, &self.config.agent_name)
            .map_err(|e| SupervisorError::Store(e.to_string()))?;

        let msg = msgs.into_iter().next().ok_or(SupervisorError::NoMessages)?;

        // Atomic claim.
        let won = self
            .store
            .claim(msg.id)
            .map_err(|e| SupervisorError::Store(e.to_string()))?;
        if !won {
            return Err(SupervisorError::ClaimConflict);
        }

        // Extract prompt text from message parts.
        let prompt = parts_to_text(&msg.parts);

        // Handle Question → InputRequired transition before resuming.
        if msg.kind == MessageKind::Question {
            if let Some(task_id) = self.task_id {
                let _ = self.store.task_update(
                    task_id,
                    MailboxTaskState::InputRequired,
                    Some("question received"),
                );
            }
        }

        // Resume engine; on resume-400 retry once with stripped context.
        let resume_result = self.engine.resume(conv_id, &prompt).await;
        match resume_result {
            Err(e) if is_resume_400(&e) => {
                let fallback = format!("[context stripped]\n{prompt}");
                self.engine
                    .resume(conv_id, &fallback)
                    .await
                    .map_err(|e2| SupervisorError::Engine(e2.to_string()))?;
            }
            Err(e) => return Err(SupervisorError::Engine(e.to_string())),
            Ok(_) => {}
        }

        // Mark the message consumed.
        self.store
            .consume(msg.id)
            .map_err(|e| SupervisorError::Store(e.to_string()))?;

        // After a Question the task reverts to Working once the answer was pumped.
        if msg.kind == MessageKind::Question {
            if let Some(task_id) = self.task_id {
                let _ = self.store.task_update(
                    task_id,
                    MailboxTaskState::Working,
                    Some("answer consumed"),
                );
            }
        }

        Ok(())
    }

    /// Drive the loop for up to `max_turns`, stopping on [`SupervisorError::NoMessages`].
    ///
    /// Returns the number of turns successfully pumped.
    pub async fn pump_loop(&mut self, max_turns: usize) -> Result<usize, SupervisorError> {
        let mut pumped = 0;
        for _ in 0..max_turns {
            match self.pump_one().await {
                Ok(()) => pumped += 1,
                Err(SupervisorError::NoMessages) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(pumped)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn parts_to_text(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(|p| {
            if let Part::Text { text } = p {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_resume_400(e: &substrate_core::error::SubstrateError) -> bool {
    e.to_string().contains("resume-400")
        || e.to_string().contains("reasoning_details not permitted")
}
