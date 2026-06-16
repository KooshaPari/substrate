//! Transport conformance: publish/subscribe round-trip and atomic claim.

use substrate_core::domain::{Message, MessageKind, Part};
use substrate_core::ports::TransportPort;
use substrate_core::SubstrateError;
use transport_file::FileTransport;
use uuid::Uuid;

fn msg(to: &str) -> Message {
    Message {
        id: Uuid::new_v4(),
        from: "lead".into(),
        to: to.into(),
        kind: MessageKind::Task,
        parts: vec![Part::Text { text: "work".into() }],
        in_reply_to: None,
    }
}

#[tokio::test]
async fn publish_then_subscribe_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let t = FileTransport::new(dir.path()).unwrap();
    let m = msg("worker");
    t.publish(&m).await.unwrap();

    let got = t.subscribe("worker").await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, m.id);
}

#[tokio::test]
async fn claim_is_atomic_second_claim_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let t = FileTransport::new(dir.path()).unwrap();
    let m = msg("worker");
    t.publish(&m).await.unwrap();

    let first = t.claim("worker", &m.id).await;
    assert!(first.is_ok());

    let second = t.claim("worker", &m.id).await;
    assert!(matches!(second, Err(SubstrateError::ClaimConflict(_))));
}

#[tokio::test]
async fn claim_missing_message_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let t = FileTransport::new(dir.path()).unwrap();
    let res = t.claim("worker", &Uuid::new_v4()).await;
    assert!(matches!(res, Err(SubstrateError::NotFound(_))));
}
