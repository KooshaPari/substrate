//! `SqliteMailboxStore`: the SQLite-backed `MailboxStore` implementation.

use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use a2a::message::{Message, MessageKind, MsgState, Part};
use a2a::task::{Task, TaskState};
use substrate_core::mailbox_port::{MailboxStore, MailboxTaskState};

use crate::error::StoreError;
use crate::schema;

/// A `MailboxStore` backed by a SQLite database.
///
/// Thread safety: a `Mutex<Connection>` serialises all writes, which is sufficient
/// for the atomic-claim guarantee (the `UPDATE WHERE state='unread'` rowcount test).
pub struct SqliteMailboxStore {
    conn: Mutex<Connection>,
}

impl SqliteMailboxStore {
    /// Open (or create) a store at the given file path.
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open a transient in-memory store (useful in tests).
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn task_state_to_str(s: MailboxTaskState) -> &'static str {
    match s {
        MailboxTaskState::Submitted => "submitted",
        MailboxTaskState::Working => "working",
        MailboxTaskState::InputRequired => "input_required",
        MailboxTaskState::Completed => "completed",
        MailboxTaskState::Failed => "failed",
        MailboxTaskState::Cancelled => "cancelled",
    }
}

fn str_to_task_state(s: &str) -> TaskState {
    match s {
        "working" => TaskState::Working,
        "input_required" => TaskState::InputRequired,
        "completed" => TaskState::Completed,
        "failed" => TaskState::Failed,
        "cancelled" => TaskState::Cancelled,
        _ => TaskState::Submitted,
    }
}

fn msg_kind_to_str(k: &MessageKind) -> &'static str {
    match k {
        MessageKind::Task => "task",
        MessageKind::Reply => "reply",
        MessageKind::Question => "question",
        MessageKind::Status => "status",
        MessageKind::Artifact => "artifact",
    }
}

fn str_to_msg_kind(s: &str) -> MessageKind {
    match s {
        "reply" => MessageKind::Reply,
        "question" => MessageKind::Question,
        "status" => MessageKind::Status,
        "artifact" => MessageKind::Artifact,
        _ => MessageKind::Task,
    }
}

fn a2a_task_state_to_str(s: TaskState) -> &'static str {
    match s {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::InputRequired => "input_required",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
    }
}

// ── MailboxStore impl ─────────────────────────────────────────────────────────

impl MailboxStore for SqliteMailboxStore {
    type Msg = Message;
    type Task = Task;
    type Error = StoreError;

    fn post(&self, msg: &Message) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        let parts_json = serde_json::to_string(&msg.parts)?;
        conn.execute(
            "INSERT INTO mailbox \
             (id, team_id, task_id, from_agent, to_agent, kind, parts, in_reply_to, state, created_at, consumed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                msg.id.to_string(),
                msg.team_id,
                msg.task_id.map(|u| u.to_string()),
                msg.from,
                msg.to,
                msg_kind_to_str(&msg.kind),
                parts_json,
                msg.in_reply_to.map(|u| u.to_string()),
                "unread",
                msg.created_at.to_rfc3339(),
                msg.consumed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    fn inbox(&self, team_id: &str, to: &str) -> Result<Vec<Message>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, task_id, from_agent, to_agent, kind, parts, \
                    in_reply_to, state, created_at, consumed_at \
             FROM mailbox \
             WHERE team_id=?1 AND to_agent=?2 AND state='unread' \
             ORDER BY created_at ASC",
        )?;
        let rows: Vec<Message> = stmt
            .query_map(params![team_id, to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(
                    id_str,
                    team_id,
                    task_id_str,
                    from,
                    to,
                    kind_str,
                    parts_json,
                    in_reply_str,
                    state_str,
                    created_str,
                    consumed_str,
                )| {
                    let parts: Vec<Part> = serde_json::from_str(&parts_json).unwrap_or_default();
                    let state = match state_str.as_str() {
                        "delivered" => MsgState::Delivered,
                        "consumed" => MsgState::Consumed,
                        _ => MsgState::Unread,
                    };
                    Message {
                        id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                        team_id,
                        task_id: task_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                        from,
                        to,
                        kind: str_to_msg_kind(&kind_str),
                        parts,
                        in_reply_to: in_reply_str.and_then(|s| Uuid::parse_str(&s).ok()),
                        state,
                        created_at: created_str.parse().unwrap_or_else(|_| Utc::now()),
                        consumed_at: consumed_str.and_then(|s| s.parse().ok()),
                    }
                },
            )
            .collect();
        Ok(rows)
    }

    fn claim(&self, message_id: Uuid) -> Result<bool, StoreError> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE mailbox SET state='delivered' WHERE id=?1 AND state='unread'",
            params![message_id.to_string()],
        )?;
        Ok(n == 1)
    }

    fn consume(&self, message_id: Uuid) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE mailbox SET state='consumed', consumed_at=?2 WHERE id=?1",
            params![message_id.to_string(), Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    fn task_create(&self, task: &Task) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasklist \
             (id, team_id, title, state, owner, parent_task_id, requirement_id, epic_id, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                task.id.to_string(),
                task.team_id,
                task.title,
                a2a_task_state_to_str(task.state),
                task.owner,
                task.parent_task_id.map(|u| u.to_string()),
                task.requirement_id,
                task.epic_id,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    fn task_update(
        &self,
        id: Uuid,
        state: MailboxTaskState,
        note: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasklist SET state=?2, updated_at=?3, note=?4 WHERE id=?1",
            params![
                id.to_string(),
                task_state_to_str(state),
                Utc::now().to_rfc3339(),
                note,
            ],
        )?;
        Ok(())
    }

    fn task_list(&self, team_id: &str) -> Result<Vec<Task>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, team_id, title, state, owner, parent_task_id, requirement_id, epic_id, created_at, updated_at \
             FROM tasklist WHERE team_id=?1 ORDER BY created_at ASC",
        )?;
        let tasks: Vec<Task> = stmt
            .query_map(params![team_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(
                    id_str,
                    team_id,
                    title,
                    state_str,
                    owner,
                    parent_str,
                    req_id,
                    epic_id,
                    created_str,
                    updated_str,
                )| Task {
                    id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
                    team_id,
                    title,
                    state: str_to_task_state(&state_str),
                    owner,
                    parent_task_id: parent_str.and_then(|s| Uuid::parse_str(&s).ok()),
                    requirement_id: req_id,
                    epic_id,
                    created_at: created_str.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: updated_str.parse().unwrap_or_else(|_| Utc::now()),
                },
            )
            .collect();
        Ok(tasks)
    }
}
