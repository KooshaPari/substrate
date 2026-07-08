use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

pub struct ShardMap2<K: Hash + Eq, V> { shards: Vec<HashMap<K, V>>, count: usize }
impl<K: Hash + Eq + Clone, V> ShardMap2<K, V> {
    pub fn new(shard_count: usize) -> Self {
        let n = shard_count.max(1);
        let mut shards = Vec::with_capacity(n);
        for _ in 0..n { shards.push(HashMap::new()); }
        Self { shards, count: n }
    }
    fn shard_idx<Q: Hash + ?Sized>(&self, k: &Q) -> usize {
        let mut h = DefaultHasher::new();
        k.hash(&mut h);
        (h.finish() as usize) % self.count
    }
    pub fn insert(&mut self, k: K, v: V) -> Option<V> { let idx = self.shard_idx(&k); self.shards[idx].insert(k, v) }
    pub fn get<Q>(&self, k: &Q) -> Option<&V> where K: std::borrow::Borrow<Q>, Q: Hash + Eq + ?Sized { self.shards[self.shard_idx(k)].get(k) }
    pub fn remove<Q>(&mut self, k: &Q) -> Option<V> where K: std::borrow::Borrow<Q>, Q: Hash + Eq + ?Sized { let idx = self.shard_idx(k); self.shards[idx].remove(k) }
    pub fn total_len(&self) -> usize { self.shards.iter().map(|s| s.len()).sum() }
    pub fn shard_count(&self) -> usize { self.count }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn insert_get() { let mut m = ShardMap2::new(4); m.insert("a", 1); assert_eq!(m.get("a"), Some(&1)); }
    #[test] fn remove() { let mut m = ShardMap2::new(4); m.insert("a", 1); assert_eq!(m.remove("a"), Some(1)); }
    #[test] fn total_len() { let mut m = ShardMap2::new(4); m.insert("a", 1); m.insert("b", 2); assert_eq!(m.total_len(), 2); }
    #[test] fn min_one() { let m = ShardMap2::<&str, i32>::new(0); assert_eq!(m.shard_count(), 1); }
    #[test] fn overwrites() { let mut m = ShardMap2::new(4); m.insert("k", 1); m.insert("k", 2); assert_eq!(m.get("k"), Some(&2)); }
}
