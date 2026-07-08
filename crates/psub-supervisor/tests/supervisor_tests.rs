//! Integration tests for the supervisor crate.

use std::sync::Arc;

use psub_a2a::message::{Message, MessageKind, Part};
use psub_a2a::task::Task as A2aTask;
use store_sqlite::SqliteMailboxStore;
use substrate_core::mailbox_port::MailboxStore;
use psub_supervisor::{FakeEngine, FakeResponse, LaneConfig, Supervisor};

/// Helper: build a text message addressed to `agent` in `team`.
fn make_msg(team_id: &str, to: &str, kind: MessageKind, text: &str) -> Message {
    Message::new(
        team_id,
        "test-sender",
        to,
        kind,
        vec![Part::Text { text: text.into() }],
    )
}

// ── 1. Basic turn pump ────────────────────────────────────────────────────────

/// Spawn the engine, post a Task message, pump_one → message is consumed.
#[tokio::test]
async fn test_turn_pump() {
    let engine = Arc::new(FakeEngine::new());
    let store = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());
    let config = LaneConfig::new("team-turn", "agent-a");
    let mut sup = Supervisor::new(engine, Arc::clone(&store), config);

    sup.spawn("hello world").await.unwrap();

    // Post a task message to the agent's inbox.
    let msg = make_msg("team-turn", "agent-a", MessageKind::Task, "do some work");
    store.post(&msg).unwrap();

    // Inbox has 1 message.
    let inbox = store.inbox("team-turn", "agent-a").unwrap();
    assert_eq!(inbox.len(), 1);

    // pump_one should succeed.
    sup.pump_one().await.unwrap();

    // After consuming, inbox is empty.
    let inbox_after = store.inbox("team-turn", "agent-a").unwrap();
    assert_eq!(inbox_after.len(), 0, "message should be consumed");
}

// ── 2. Question → InputRequired ───────────────────────────────────────────────

/// A Question message causes the task to become InputRequired; supervisor
/// does not block and a follow-up Reply gets pumped successfully.
#[tokio::test]
async fn test_question_input_required() {
    let engine = Arc::new(FakeEngine::new());
    let store = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());
    let config = LaneConfig::new("team-q", "agent-b");
    let mut sup = Supervisor::new(Arc::clone(&engine), Arc::clone(&store), config);

    sup.spawn("start task").await.unwrap();

    // Post a Question message.
    let q = make_msg("team-q", "agent-b", MessageKind::Question, "which branch?");
    store.post(&q).unwrap();

    // pump_one processes the question (engine resumes, task→InputRequired→Working).
    sup.pump_one().await.unwrap();

    // Tasks list shows the task in Working state (reverted after answer consumed).
    let tasks = store.task_list("team-q").unwrap();
    assert!(!tasks.is_empty(), "task record should exist");

    // Post an answer Reply.
    let r = make_msg("team-q", "agent-b", MessageKind::Reply, "use main");
    store.post(&r).unwrap();

    // Second pump succeeds.
    sup.pump_one().await.unwrap();

    // Inbox is empty.
    assert_eq!(store.inbox("team-q", "agent-b").unwrap().len(), 0);
}

// ── 3. Hierarchy task tree ────────────────────────────────────────────────────

/// Three-level task hierarchy: lead → teammate → subagent tasks all stored.
#[tokio::test]
async fn test_hierarchy_task_tree() {
    let store = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());

    // Lead task.
    let lead_task = A2aTask::new("team-h", "lead task", "lead");
    let lead_id = lead_task.id;
    store.task_create(&lead_task).unwrap();

    // Teammate task (child of lead).
    let mut tm_task = A2aTask::new("team-h", "teammate task", "teammate");
    tm_task.parent_task_id = Some(lead_id);
    let tm_id = tm_task.id;
    store.task_create(&tm_task).unwrap();

    // Subagent task (child of teammate).
    let mut sub_task = A2aTask::new("team-h", "subagent task", "subagent");
    sub_task.parent_task_id = Some(tm_id);
    store.task_create(&sub_task).unwrap();

    let tasks = store.task_list("team-h").unwrap();
    assert_eq!(tasks.len(), 3);

    let lead = tasks.iter().find(|t| t.owner == "lead").unwrap();
    let tm = tasks.iter().find(|t| t.owner == "teammate").unwrap();
    let sub = tasks.iter().find(|t| t.owner == "subagent").unwrap();

    assert_eq!(lead.parent_task_id, None);
    assert_eq!(tm.parent_task_id, Some(lead_id));
    assert_eq!(sub.parent_task_id, Some(tm_id));
}

// ── 4. Atomic claim / race ────────────────────────────────────────────────────

/// Two threads race to claim the same message; exactly one wins.
#[tokio::test]
async fn test_atomic_claim_restart() {
    let store = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());
    let msg = make_msg("team-race", "agent-c", MessageKind::Task, "race me");
    store.post(&msg).unwrap();

    let store1 = Arc::clone(&store);
    let store2 = Arc::clone(&store);
    let id = msg.id;

    // Spawn two tasks racing to claim.
    let h1 = tokio::spawn(async move { store1.claim(id) });
    let h2 = tokio::spawn(async move { store2.claim(id) });

    let r1 = h1.await.unwrap().unwrap();
    let r2 = h2.await.unwrap().unwrap();

    // Exactly one of them should have won.
    assert!(
        r1 ^ r2,
        "exactly one thread should win the claim (r1={r1}, r2={r2})"
    );
}

// ── 5. Resume-400 fallback ────────────────────────────────────────────────────

/// When the engine returns a resume-400 error on first try, the supervisor
/// retries with a stripped context prefix and succeeds. Task state survives.
#[tokio::test]
async fn test_resume_400_fallback() {
    let engine = Arc::new(FakeEngine::new());
    // Script: start succeeds, first resume → 400, second resume → Ok.
    engine.push(FakeResponse::Ok("started".into()));
    engine.push(FakeResponse::Resume400);
    engine.push(FakeResponse::Ok("recovered".into()));

    let store = Arc::new(SqliteMailboxStore::open_in_memory().unwrap());
    let config = LaneConfig::new("team-400", "agent-d");
    let mut sup = Supervisor::new(Arc::clone(&engine), Arc::clone(&store), config);

    sup.spawn("initial prompt").await.unwrap();
    assert!(sup.conv_id().is_some(), "conv_id should be set after spawn");

    // Post a message that will trigger the 400 path.
    let msg = make_msg("team-400", "agent-d", MessageKind::Reply, "continue");
    store.post(&msg).unwrap();

    // pump_one should not error — the retry succeeds.
    sup.pump_one()
        .await
        .expect("pump_one should recover from resume-400");

    // Inbox consumed.
    assert_eq!(store.inbox("team-400", "agent-d").unwrap().len(), 0);

    // Task record is still intact.
    let tasks = store.task_list("team-400").unwrap();
    assert!(
        !tasks.is_empty(),
        "task record should survive the 400 retry"
    );

    // Two engine calls were made for the one pump_one (400 + retry).
    let calls = *engine.call_count.lock().unwrap();
    // 1 for spawn start + 2 for pump_one (400 + retry) = 3.
    assert_eq!(calls, 3, "expected 1 start + 2 resume calls, got {calls}");
}

// ── 6. Crash/restart recovery ────────────────────────────────────────────────

/// A restarted supervisor can rehydrate its active task from the SQLite tasklist
/// and continue pumping mailbox messages without calling spawn again.
#[tokio::test]
async fn test_restart_recovers_active_task_from_sqlite() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db_path = tmp.path().to_string_lossy().to_string();
    let engine = Arc::new(FakeEngine::new());

    let recovered_conv_id = {
        let store = Arc::new(SqliteMailboxStore::open(&db_path).unwrap());
        let config = LaneConfig::new("team-recover", "agent-r");
        let mut sup = Supervisor::new(Arc::clone(&engine), Arc::clone(&store), config);

        sup.spawn("long running task").await.unwrap();
        let conv_id = sup.conv_id().unwrap().to_string();

        let msg = make_msg(
            "team-recover",
            "agent-r",
            MessageKind::Task,
            "continue after restart",
        );
        store.post(&msg).unwrap();

        conv_id
    };

    let store = Arc::new(SqliteMailboxStore::open(&db_path).unwrap());
    let config = LaneConfig::new("team-recover", "agent-r");
    let mut restarted = Supervisor::new(Arc::clone(&engine), Arc::clone(&store), config);

    assert!(
        restarted.recover_active().unwrap(),
        "active task should be recovered from sqlite"
    );
    assert_eq!(restarted.conv_id(), Some(recovered_conv_id.as_str()));

    restarted.pump_one().await.unwrap();

    assert_eq!(store.inbox("team-recover", "agent-r").unwrap().len(), 0);

    let tasks = store.task_list("team-recover").unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].owner, "agent-r");
}
