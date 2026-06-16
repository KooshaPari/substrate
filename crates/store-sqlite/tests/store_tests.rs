use std::sync::Arc;
use std::thread;

use a2a::message::{Message, MessageKind, Part};
use a2a::task::Task;
use store_sqlite::SqliteMailboxStore;
use substrate_core::mailbox_port::{MailboxStore, MailboxTaskState};

fn make_store() -> SqliteMailboxStore {
    SqliteMailboxStore::open_in_memory().expect("in-memory store")
}

#[test]
fn inbox_returns_unread_for_correct_recipient() {
    let store = make_store();
    let msg = Message::new(
        "team-a",
        "lead",
        "worker-1",
        MessageKind::Task,
        vec![Part::Text { text: "go".into() }],
    );
    let msg2 = Message::new(
        "team-a",
        "lead",
        "worker-2",
        MessageKind::Task,
        vec![Part::Text {
            text: "other".into(),
        }],
    );

    store.post(&msg).unwrap();
    store.post(&msg2).unwrap();

    let inbox = store.inbox("team-a", "worker-1").unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].to, "worker-1");
}

#[test]
fn atomic_claim_no_double_consume() {
    let store = Arc::new(make_store());
    let msg = Message::new(
        "team-b",
        "lead",
        "worker",
        MessageKind::Status,
        vec![Part::Text {
            text: "status".into(),
        }],
    );
    let msg_id = msg.id;
    store.post(&msg).unwrap();

    // Spawn 4 threads all trying to claim the same message
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let s = Arc::clone(&store);
            thread::spawn(move || s.claim(msg_id).unwrap_or(false))
        })
        .collect();

    let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let wins = results.iter().filter(|&&b| b).count();
    assert_eq!(
        wins, 1,
        "exactly one thread should win the claim race, got {wins}"
    );
}

#[test]
fn task_tree_three_levels() {
    let store = make_store();

    let parent = Task::new("team-c", "parent task", "lead");
    let mut child = Task::new("team-c", "child task", "worker-1");
    child.parent_task_id = Some(parent.id);
    let mut grandchild = Task::new("team-c", "grandchild task", "worker-2");
    grandchild.parent_task_id = Some(child.id);

    store.task_create(&parent).unwrap();
    store.task_create(&child).unwrap();
    store.task_create(&grandchild).unwrap();

    let tasks = store.task_list("team-c").unwrap();
    assert_eq!(tasks.len(), 3);

    let gc = tasks.iter().find(|t| t.title == "grandchild task").unwrap();
    assert_eq!(gc.parent_task_id, Some(child.id));

    let ch = tasks.iter().find(|t| t.title == "child task").unwrap();
    assert_eq!(ch.parent_task_id, Some(parent.id));
}

#[test]
fn task_update_advances_state() {
    let store = make_store();
    let task = Task::new("team-d", "update test", "lead");
    let task_id = task.id;
    store.task_create(&task).unwrap();

    store
        .task_update(task_id, MailboxTaskState::Working, Some("started"))
        .unwrap();

    let tasks = store.task_list("team-d").unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].state, a2a::task::TaskState::Working);
}

#[test]
fn consume_marks_message_consumed() {
    let store = make_store();
    let msg = Message::new(
        "team-e",
        "lead",
        "worker",
        MessageKind::Reply,
        vec![Part::Text {
            text: "done".into(),
        }],
    );
    let msg_id = msg.id;
    store.post(&msg).unwrap();
    store.claim(msg_id).unwrap();
    store.consume(msg_id).unwrap();

    // After consumption, inbox for worker should be empty (only returns unread)
    let inbox = store.inbox("team-e", "worker").unwrap();
    assert!(inbox.is_empty());
}
