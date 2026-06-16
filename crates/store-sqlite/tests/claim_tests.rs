use std::sync::Arc;
use std::thread;

use store_sqlite::SqliteClaimStore;
use substrate_core::claim_port::ClaimPort;

fn make_store() -> SqliteClaimStore {
    SqliteClaimStore::open_in_memory().expect("in-memory claim store")
}

#[test]
fn atomic_claim_exactly_one_winner() {
    let store = Arc::new(make_store());
    let item_id = store.enqueue("q-a", "process payment refund").unwrap();

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let s = Arc::clone(&store);
            thread::spawn(move || s.claim_next("q-a", &format!("worker-{i}")).unwrap())
        })
        .collect();

    let claims: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let winners: Vec<_> = claims.into_iter().flatten().collect();
    assert_eq!(
        winners.len(),
        1,
        "exactly one thread should win the claim race"
    );
    assert_eq!(winners[0].id, item_id);
}

#[test]
fn near_duplicate_enqueue_rejected() {
    let store = make_store();
    store
        .enqueue("q-b", "refactor auth module login flow")
        .unwrap();
    let dup = store.enqueue("q-b", "refactor auth module login flo");
    assert!(dup.is_err(), "near-duplicate should be rejected");
}

#[test]
fn distinct_bodies_both_enqueue() {
    let store = make_store();
    store.enqueue("q-c", "add metrics dashboard").unwrap();
    store.enqueue("q-c", "write integration tests").unwrap();
    let first = store.claim_next("q-c", "w1").unwrap();
    assert!(first.is_some());
}
