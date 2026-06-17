//! Tests for `SqliteConfigStore`.

use store_sqlite::SqliteConfigStore;

#[test]
fn set_get_round_trip() {
    let store = SqliteConfigStore::open_in_memory().unwrap();
    store.set("routing.default_model", "kimi").unwrap();
    let entry = store.get("routing.default_model").unwrap().unwrap();
    assert_eq!(entry.key, "routing.default_model");
    assert_eq!(entry.value, "kimi");
}

#[test]
fn set_overwrites_existing_key() {
    let store = SqliteConfigStore::open_in_memory().unwrap();
    store.set("k", "v1").unwrap();
    store.set("k", "v2").unwrap();
    let entry = store.get("k").unwrap().unwrap();
    assert_eq!(entry.value, "v2");
}

#[test]
fn delete_removes_key() {
    let store = SqliteConfigStore::open_in_memory().unwrap();
    store.set("k", "v").unwrap();
    assert!(store.delete("k").unwrap());
    assert!(store.get("k").unwrap().is_none());
}

#[test]
fn list_returns_all_entries() {
    let store = SqliteConfigStore::open_in_memory().unwrap();
    store.set("b", "2").unwrap();
    store.set("a", "1").unwrap();
    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "a");
    assert_eq!(entries[1].key, "b");
}
