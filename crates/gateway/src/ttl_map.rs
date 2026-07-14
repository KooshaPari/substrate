use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct TtlEntry<V> {
    pub value: V,
    pub expires_at: Instant,
}

pub struct TtlMap<K: std::hash::Hash + Eq + Clone, V> {
    entries: HashMap<K, TtlEntry<V>>,
}
impl<K: std::hash::Hash + Eq + Clone, V> TtlMap<K, V> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
    pub fn insert(&mut self, k: K, v: V, ttl: Duration) -> Option<V> {
        let expires_at = Instant::now() + ttl;
        self.entries
            .insert(
                k,
                TtlEntry {
                    value: v,
                    expires_at,
                },
            )
            .map(|e| e.value)
    }
    pub fn get(&mut self, k: &K) -> Option<&V> {
        let expired = self
            .entries
            .get(k)
            .map_or(true, |e| e.expires_at <= Instant::now());
        if expired {
            self.entries.remove(k);
            None
        } else {
            self.entries.get(k).map(|e| &e.value)
        }
    }
    pub fn remove(&mut self, k: &K) -> Option<V> {
        self.entries.remove(k).map(|e| e.value)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn purge_expired(&mut self) -> usize {
        let now = Instant::now();
        let before = self.entries.len();
        self.entries.retain(|_, v| v.expires_at > now);
        before - self.entries.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insert_and_get() {
        let mut m: TtlMap<&str, i32> = TtlMap::new();
        m.insert("a", 1, Duration::from_secs(60));
        assert_eq!(m.get(&"a"), Some(&1));
    }
    #[test]
    fn expired_gets_none() {
        let mut m: TtlMap<&str, i32> = TtlMap::new();
        m.insert("a", 1, Duration::ZERO);
        assert_eq!(m.get(&"a"), None);
    }
    #[test]
    fn remove() {
        let mut m: TtlMap<&str, i32> = TtlMap::new();
        m.insert("a", 1, Duration::from_secs(60));
        assert_eq!(m.remove(&"a"), Some(1));
        assert!(m.is_empty());
    }
    #[test]
    fn purge() {
        let mut m: TtlMap<&str, i32> = TtlMap::new();
        m.insert("a", 1, Duration::ZERO);
        m.insert("b", 2, Duration::from_secs(60));
        let purged = m.purge_expired();
        assert_eq!(purged, 1);
        assert_eq!(m.len(), 1);
    }
    #[test]
    fn insert_overwrites() {
        let mut m: TtlMap<&str, i32> = TtlMap::new();
        m.insert("a", 1, Duration::from_secs(60));
        m.insert("a", 2, Duration::from_secs(60));
        assert_eq!(m.get(&"a"), Some(&2));
    }
}
