//! Store conformance: persist/load round-trip and atomic claim.

use store_file::FileStore;
use substrate_core::domain::{Task, TaskState};
use substrate_core::ports::StorePort;
use substrate_core::SubstrateError;

#[tokio::test]
async fn persist_then_load_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let s = FileStore::new(dir.path()).unwrap();
    let task = Task::new("prompt", "/cwd");
    s.persist(&task).await.unwrap();

    let got = s.load(&task.id).await.unwrap();
    assert_eq!(got, task);
}

#[tokio::test]
async fn claim_atomic_advances_and_second_claim_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let s = FileStore::new(dir.path()).unwrap();
    let task = Task::new("prompt", "/cwd");
    s.persist(&task).await.unwrap();

    let claimed = s.claim_atomic(&task.id).await.unwrap();
    assert_eq!(claimed.state, TaskState::Working);

    let second = s.claim_atomic(&task.id).await;
    assert!(matches!(second, Err(SubstrateError::ClaimConflict(_))));
}

#[tokio::test]
async fn load_missing_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let s = FileStore::new(dir.path()).unwrap();
    let res = s.load(&uuid::Uuid::new_v4()).await;
    assert!(matches!(res, Err(SubstrateError::NotFound(_))));
}
