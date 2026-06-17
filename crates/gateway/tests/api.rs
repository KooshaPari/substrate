//! Integration tests for the substrate gateway (offline, hexagonal fakes).

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use gateway::{build_router, test_state, AppState};
use http_body_util::BodyExt;
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::error::Result;
use substrate_core::ports::RoutingPort;
use tower::ServiceExt;

/// Fake [`RoutingPort`] for deterministic offline tests.
#[derive(Debug, Clone, Default)]
struct FakeRouter {
    model: String,
}

impl FakeRouter {
    fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }
}

#[async_trait]
impl RoutingPort for FakeRouter {
    async fn route_decision(&self, _task: &Task) -> Result<RoutingDecision> {
        Ok(RoutingDecision {
            engine: "forge".to_string(),
            model: self.model.clone(),
            reason: Some("fake-router".to_string()),
        })
    }
}

fn fake_state(tmp: &tempfile::TempDir) -> AppState {
    let routing: Arc<dyn RoutingPort> = Arc::new(FakeRouter::with_model("test-model"));
    test_state(tmp.path(), routing).unwrap()
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned()))
}

#[tokio::test]
async fn healthz_returns_200() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn models_lists_routed_model() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let data = json["data"].as_array().expect("models data array");
    assert!(!data.is_empty());
    assert_eq!(data[0]["id"], "test-model");
    assert_eq!(data[0]["object"], "model");
}

#[tokio::test]
async fn chat_completions_routes_via_routing_port() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"auto","messages":[{"role":"user","content":"hello"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["object"], "chat.completion");
    assert_eq!(json["model"], "test-model");
    assert!(json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .contains("test-model"));
}

#[tokio::test]
async fn chat_completions_rejects_empty_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model":"auto","messages":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn management_config_set_get_list_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let set_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/management/config")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"action":"set","key":"routing.mode","value":"round_robin"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(set_resp.status(), StatusCode::OK);

    let get_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/management/config")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"get","key":"routing.mode"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["entry"]["value"], "round_robin");

    let list_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/management/config")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"list"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_json = body_json(list_resp).await;
    assert_eq!(list_json["entries"].as_array().unwrap().len(), 1);

    let del_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/management/config")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"delete","key":"routing.mode"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn a2a_message_send_and_inbox_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let msg = serde_json::json!({
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "team_id": "alpha",
        "task_id": null,
        "from": "planner",
        "to": "worker",
        "kind": "task",
        "parts": [{"type": "text", "text": "do the thing"}],
        "in_reply_to": null,
        "state": "unread",
        "created_at": "2026-06-16T00:00:00Z",
        "consumed_at": null
    });

    let send_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/a2a/messages")
                .header("content-type", "application/json")
                .body(Body::from(msg.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(send_resp.status(), StatusCode::CREATED);

    let inbox_resp = app
        .oneshot(
            Request::builder()
                .uri("/a2a/inbox?team=alpha&to=worker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inbox_resp.status(), StatusCode::OK);
    let json = body_json(inbox_resp).await;
    let msgs = json.as_array().expect("inbox array");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["from"], "planner");
}

#[tokio::test]
async fn a2a_tasks_list_requires_team() {
    let tmp = tempfile::tempdir().unwrap();
    let app = build_router(fake_state(&tmp));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/a2a/tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
