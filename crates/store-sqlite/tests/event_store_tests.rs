use std::sync::Arc;
use std::thread;

use store_sqlite::SqliteEventStore;
use substrate_core::domain::TaskState;
use substrate_core::event_store_port::{replay_task_state, EventStorePort, TaskLifecycleEvent};
use uuid::Uuid;

fn make_store() -> SqliteEventStore<TaskLifecycleEvent> {
    SqliteEventStore::open_in_memory().expect("in-memory event store")
}

#[test]
fn append_n_events_load_returns_ordered_with_monotonic_seq() {
    let store = make_store();
    let id = Uuid::new_v4();

    store
        .append(
            id,
            0,
            &TaskLifecycleEvent::Created {
                prompt: "ship feature".into(),
                cwd: "/repo".into(),
            },
        )
        .unwrap();
    store
        .append(
            id,
            1,
            &TaskLifecycleEvent::Advanced {
                to: TaskState::Working,
            },
        )
        .unwrap();
    store
        .append(
            id,
            2,
            &TaskLifecycleEvent::Advanced {
                to: TaskState::Completed,
            },
        )
        .unwrap();

    let loaded = store.load(id).unwrap();
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].aggregate_seq, 0);
    assert_eq!(loaded[1].aggregate_seq, 1);
    assert_eq!(loaded[2].aggregate_seq, 2);
    assert!(loaded[0].global_seq < loaded[1].global_seq);
    assert!(loaded[1].global_seq < loaded[2].global_seq);
}

#[test]
fn replay_folds_events_into_correct_final_state() {
    let store = make_store();
    let id = Uuid::new_v4();

    store
        .append(
            id,
            0,
            &TaskLifecycleEvent::Created {
                prompt: "replay me".into(),
                cwd: "/tmp".into(),
            },
        )
        .unwrap();
    store
        .append(
            id,
            1,
            &TaskLifecycleEvent::Advanced {
                to: TaskState::Working,
            },
        )
        .unwrap();
    store
        .append(
            id,
            2,
            &TaskLifecycleEvent::Advanced {
                to: TaskState::Completed,
            },
        )
        .unwrap();

    let state = replay_task_state(&store, id).unwrap();
    assert_eq!(state.prompt, "replay me");
    assert_eq!(state.cwd, "/tmp");
    assert_eq!(state.state, TaskState::Completed);
}

#[test]
fn duplicate_expected_seq_append_rejected() {
    let store = make_store();
    let id = Uuid::new_v4();

    store
        .append(
            id,
            0,
            &TaskLifecycleEvent::Created {
                prompt: "once".into(),
                cwd: ".".into(),
            },
        )
        .unwrap();

    let err = store
        .append(
            id,
            0,
            &TaskLifecycleEvent::Advanced {
                to: TaskState::Working,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("duplicate event seq"));
}

#[test]
fn concurrent_appends_preserve_global_ordering() {
    let store = Arc::new(make_store());

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let s = Arc::clone(&store);
            let aggregate_id = Uuid::new_v4();
            thread::spawn(move || {
                s.append(
                    aggregate_id,
                    0,
                    &TaskLifecycleEvent::Created {
                        prompt: format!("task-{i}"),
                        cwd: ".".into(),
                    },
                )
                .unwrap()
            })
        })
        .collect();

    let mut globals: Vec<u64> = handles
        .into_iter()
        .map(|h| h.join().unwrap().global_seq)
        .collect();
    globals.sort_unstable();
    globals.dedup();
    assert_eq!(globals.len(), 8, "global_seq values must be unique");

    let all = store.load_all_global().unwrap();
    assert_eq!(all.len(), 8);
    for window in all.windows(2) {
        assert!(window[0].global_seq < window[1].global_seq);
    }
}

#[test]
fn task_lifecycle_projection_unit() {
    use substrate_core::event_store_port::{replay, TaskLifecycleProjection};

    let events = vec![
        TaskLifecycleEvent::Created {
            prompt: "p".into(),
            cwd: "c".into(),
        },
        TaskLifecycleEvent::Advanced {
            to: TaskState::Working,
        },
        TaskLifecycleEvent::Advanced {
            to: TaskState::Failed,
        },
    ];
    let state = replay::<TaskLifecycleProjection>(&events);
    assert_eq!(state.state, TaskState::Failed);
}
