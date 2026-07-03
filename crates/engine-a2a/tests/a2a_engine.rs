use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use engine_a2a::A2AEngine;
use engine_spec::TaskSpec;
use substrate_core::domain::{ConversationDump, Task, TaskState};
use substrate_core::ports::EnginePort;
use tokio::net::TcpListener;

#[test]
fn agent_url_prefers_http_cwd() {
    let engine = A2AEngine::new();
    let spec = TaskSpec::new("do work", "https://agent.example/a2a");

    let url = engine.agent_url_for(&spec).expect("agent url");

    assert_eq!(url, "https://agent.example/a2a");
}

#[test]
fn tasks_url_appends_tasks_to_agent_url() {
    let engine = A2AEngine::new();

    assert_eq!(
        engine.tasks_url("https://agent.example/a2a/"),
        "https://agent.example/a2a/tasks"
    );
}

#[test]
fn extract_result_maps_completed_a2a_task_to_completed() {
    let engine = A2AEngine::new();
    let task = a2a::Task::new("substrate", "write code", "remote-agent");
    let mut raw_task = task;
    raw_task.state = a2a::TaskState::Completed;
    let dump = ConversationDump {
        conversation_id: raw_task.id.to_string(),
        raw: serde_json::to_string(&raw_task).expect("serialize task"),
    };

    let result = engine.extract_result(&dump).expect("result");

    assert_eq!(result.status, TaskState::Completed);
}

#[tokio::test]
async fn start_posts_task_streams_events_and_polls_status() {
    #[derive(Clone)]
    struct TestState {
        post_count: Arc<AtomicUsize>,
        get_count: Arc<AtomicUsize>,
        events_count: Arc<AtomicUsize>,
    }

    async fn post_task(
        State(state): State<TestState>,
        Json(mut task): Json<a2a::Task>,
    ) -> Json<a2a::Task> {
        state.post_count.fetch_add(1, Ordering::SeqCst);
        task.state = a2a::TaskState::Working;
        Json(task)
    }

    async fn get_task(State(state): State<TestState>, Path(id): Path<String>) -> Json<a2a::Task> {
        state.get_count.fetch_add(1, Ordering::SeqCst);
        let mut task = a2a::Task::new("substrate", "remote result", "remote-agent");
        task.id = uuid::Uuid::parse_str(&id).expect("task id");
        task.state = a2a::TaskState::Completed;
        Json(task)
    }

    async fn task_events(State(state): State<TestState>) -> impl IntoResponse {
        state.events_count.fetch_add(1, Ordering::SeqCst);
        (
            [("content-type", "text/event-stream")],
            "event: artifact\ndata: {\"kind\":\"log\",\"content\":\"ok\",\"name\":null}\n\n",
        )
    }

    let state = TestState {
        post_count: Arc::new(AtomicUsize::new(0)),
        get_count: Arc::new(AtomicUsize::new(0)),
        events_count: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/tasks", post(post_task))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/events", get(task_events))
        .with_state(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr: SocketAddr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server");
    });

    let engine = A2AEngine::new().with_polling(Duration::from_millis(1), 2);
    let task = Task::new("remote result", format!("http://{addr}"));

    let session = engine.start(&task).await.expect("start");
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert_eq!(session.conv_id, task.id.to_string());
    assert_eq!(state.post_count.load(Ordering::SeqCst), 1);
    assert!(state.get_count.load(Ordering::SeqCst) >= 1);
    assert_eq!(state.events_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn conformance_suite_passes() {
    let engine = A2AEngine::new();
    engine_conformance::assert_engine_conformance(&engine).await;
}
