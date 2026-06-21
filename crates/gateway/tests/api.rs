//! Integration tests for the substrate gateway (offline, hexagonal fakes).

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use bytes::Bytes;
use futures::stream;
use gateway::{build_router, test_state, AppState, UpstreamClient};
use http_body_util::BodyExt;
use serde_json::Value;
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

fn fake_state_with_upstream(tmp: &tempfile::TempDir, upstream: UpstreamClient) -> AppState {
    fake_state(tmp).with_upstream(upstream)
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned()))
}

#[derive(Clone)]
enum MockBehavior {
    Streaming {
        chunks: Vec<&'static str>,
    },
    Status {
        status: StatusCode,
        body: &'static str,
    },
}

#[derive(Clone)]
struct MockUpstreamState {
    captured: Arc<Mutex<Vec<Value>>>,
    auth_headers: Arc<Mutex<Vec<Option<String>>>>,
    request_count: Arc<AtomicUsize>,
    behavior: MockBehavior,
}

async fn mock_upstream_handler(
    State(state): State<MockUpstreamState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    state.request_count.fetch_add(1, Ordering::SeqCst);
    state.captured.lock().unwrap().push(body);
    state
        .auth_headers
        .lock()
        .unwrap()
        .push(headers.get(axum::http::header::AUTHORIZATION).map(|value| {
            value
                .to_str()
                .unwrap_or("<invalid-authorization-header>")
                .to_string()
        }));

    match state.behavior.clone() {
        MockBehavior::Streaming { chunks } => {
            let stream = stream::iter(
                chunks
                    .into_iter()
                    .map(|chunk| Ok::<Bytes, std::io::Error>(Bytes::from(chunk))),
            );
            let body = Body::from_stream(stream);
            let mut builder = axum::http::Response::builder().status(StatusCode::OK);
            builder = builder.header(axum::http::header::CONTENT_TYPE, "text/event-stream");
            builder = builder.header(axum::http::header::CACHE_CONTROL, "no-cache");
            builder = builder.header(axum::http::header::TRANSFER_ENCODING, "chunked");
            builder.body(body).unwrap()
        }
        MockBehavior::Status { status, body } => {
            let _ = headers;
            (status, body).into_response()
        }
    }
}

async fn spawn_mock_upstream(
    behavior: MockBehavior,
) -> (
    String,
    Arc<Mutex<Vec<Value>>>,
    Arc<Mutex<Vec<Option<String>>>>,
    Arc<AtomicUsize>,
) {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let auth_headers = Arc::new(Mutex::new(Vec::new()));
    let request_count = Arc::new(AtomicUsize::new(0));
    let state = MockUpstreamState {
        captured: captured.clone(),
        auth_headers: auth_headers.clone(),
        request_count: request_count.clone(),
        behavior,
    };

    let app = Router::new()
        .route("/v1/chat/completions", post(mock_upstream_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (
        format!("http://{addr}/v1"),
        captured,
        auth_headers,
        request_count,
    )
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
async fn chat_completions_forwards_request_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let (base_url, captured, auth_headers, _requests) =
        spawn_mock_upstream(MockBehavior::Streaming {
            chunks: vec!["data: {\"id\":\"chunk-1\"}\n\n"],
        })
        .await;
    let app = build_router(fake_state_with_upstream(
        &tmp,
        UpstreamClient::new(base_url, "upstream-key"),
    ));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}],"metadata":{"trace":"abc-123"},"temperature":0.2}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let received = captured.lock().unwrap();
    assert_eq!(received.len(), 1);
    let forwarded = &received[0];
    assert_eq!(forwarded["model"], "gpt-4o");
    assert_eq!(forwarded["messages"][0]["role"], "user");
    assert_eq!(forwarded["messages"][0]["content"], "hello");
    assert_eq!(forwarded["metadata"]["trace"], "abc-123");
    assert_eq!(forwarded["temperature"], 0.2);
    assert_eq!(forwarded["stream"], true);
    let auth = auth_headers
        .lock()
        .unwrap()
        .first()
        .cloned()
        .flatten()
        .unwrap();
    assert_eq!(auth, "Bearer upstream-key");
}

#[tokio::test]
async fn chat_completions_streams_sse_passthrough() {
    let tmp = tempfile::tempdir().unwrap();
    let (base_url, _captured, _auth_headers, _requests) =
        spawn_mock_upstream(MockBehavior::Streaming {
            chunks: vec!["data: one\n\n", "data: two\n\n"],
        })
        .await;
    let app = build_router(fake_state_with_upstream(
        &tmp,
        UpstreamClient::new(base_url, "upstream-key"),
    ));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"gpt-4o","messages":[{"role":"user","content":"stream it"}]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "text/event-stream"
    );
    assert_eq!(
        resp.headers()
            .get(axum::http::header::TRANSFER_ENCODING)
            .unwrap()
            .to_str()
            .unwrap(),
        "chunked"
    );
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes, Bytes::from_static(b"data: one\n\ndata: two\n\n"));
}

#[tokio::test]
async fn chat_completions_circuit_breaker_opens_on_5xx() {
    let tmp = tempfile::tempdir().unwrap();
    let (base_url, _captured, _auth_headers, requests) =
        spawn_mock_upstream(MockBehavior::Status {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: "upstream blew up",
        })
        .await;
    let app = build_router(fake_state_with_upstream(
        &tmp,
        UpstreamClient::new(base_url, "upstream-key"),
    ));

    let request = || {
        Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#,
            ))
            .unwrap()
    };

    let first = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(first.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let second = app.oneshot(request()).await.unwrap();
    assert_eq!(second.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
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
