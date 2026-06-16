//! Integration tests for the substrate HTTP driver (offline, fake-forge).

use std::path::PathBuf;
use std::process::Command as StdCommand;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use driver_http::{build_router, test_state};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Resolve the clean `fake-forge` binary, building it first if absent.
fn fake_forge_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let debug_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let clean = debug_dir.join(format!("fake-forge{suffix}"));

    if !clean.exists() {
        let status = StdCommand::new(env!("CARGO"))
            .args(["build", "-p", "fake-forge"])
            .status()
            .expect("spawn cargo build -p fake-forge");
        assert!(status.success(), "cargo build -p fake-forge failed");
    }
    clean
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned()))
}

#[tokio::test]
async fn healthz_returns_200() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path(), None).unwrap();
    let app = build_router(state);

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
async fn plan_returns_dispatch_plan_without_spawning() {
    let tmp = tempfile::tempdir().unwrap();

    let state = test_state(tmp.path(), None).unwrap();
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/plan")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"engine":"forge","cwd":"/tmp","prompt":"echo hi"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["engine"], "forge");
    assert_eq!(json["session_mode"], "foreground");
    assert!(json["argv"].as_array().is_some());
}

#[tokio::test]
async fn dispatch_with_fake_forge_returns_structured_result() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = fake_forge_bin();

    let state = test_state(tmp.path(), Some(&fake)).unwrap();
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/dispatch")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"engine":"forge","cwd":"{}","prompt":"echo hi"}}"#,
                    tmp.path().to_str().unwrap().replace('\\', "\\\\")
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "completed");
    assert!(json["text"]
        .as_str()
        .unwrap_or("")
        .contains("DONE: printed hi"));
}

#[tokio::test]
async fn mailbox_send_and_inbox_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path(), None).unwrap();
    let app = build_router(state);

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
                .uri("/v1/mailbox/send")
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
                .uri("/v1/mailbox/inbox?team=alpha&to=worker")
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
    assert_eq!(msgs[0]["parts"][0]["text"], "do the thing");
}

#[tokio::test]
async fn bad_input_returns_4xx() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path(), None).unwrap();
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/plan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"cwd":"","prompt":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp).await;
    assert!(json["error"].as_str().unwrap().contains("cwd"));
}

#[tokio::test]
async fn auth_token_rejects_missing_bearer() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path(), None)
        .unwrap()
        .with_auth_token(Some("secret-token".into()));
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/plan")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"engine":"forge","cwd":"/tmp","prompt":"hi"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
