use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct TtlCache2<K, V> { entries: HashMap<K, (V, Instant)>, default_ttl: Duration }
impl<K: std::hash::Hash + Eq + Clone, V: Clone> TtlCache2<K, V> {
    pub fn new(default_ttl: Duration) -> Self { Self { entries: HashMap::new(), default_ttl } }
    pub fn put(&mut self, k: K, v: V) { self.entries.insert(k, (v, Instant::now() + self.default_ttl)); }
    pub fn put_with_ttl(&mut self, k: K, v: V, ttl: Duration) { self.entries.insert(k, (v, Instant::now() + ttl)); }
    pub fn get(&mut self, k: &K) -> Option<V> {
        let now = Instant::now();
        if let Some((_, exp)) = self.entries.get(k) { if *exp <= now { self.entries.remove(k); return None; } }
        self.entries.get(k).map(|(v, _)| v.clone())
    }
    pub fn invalidate(&mut self, k: &K) { self.entries.remove(k); }
    pub fn purge_expired(&mut self) -> usize {
        let now = Instant::now();
        let before = self.entries.len();
        self.entries.retain(|_, (_, exp)| *exp > now);
        before - self.entries.len()
    }
    pub fn len(&self) -> usize { self.entries.len() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn put_get() { let mut c = TtlCache2::new(Duration::from_secs(60)); c.put("a", 1); assert_eq!(c.get(&"a"), Some(1)); }
    #[test] fn expired() { let mut c = TtlCache2::new(Duration::ZERO); c.put("a", 1); assert_eq!(c.get(&"a"), None); }
    #[test] fn custom_ttl() { let mut c = TtlCache2::new(Duration::from_secs(60)); c.put_with_ttl("x", 7, Duration::ZERO); assert_eq!(c.get(&"x"), None); }
    #[test] fn purge() { let mut c = TtlCache2::new(Duration::ZERO); c.put("a", 1); c.put("b", 2); let purged = c.purge_expired(); assert_eq!(purged, 2); assert_eq!(c.len(), 0); }
    #[test] fn invalidate() { let mut c = TtlCache2::new(Duration::from_secs(60)); c.put("k", 1); c.invalidate(&"k"); assert_eq!(c.get(&"k"), None); }
}
