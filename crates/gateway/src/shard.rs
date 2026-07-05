use std::collections::HashMap;
use std::hash::Hash;

pub struct ShardMap<K: Hash + Eq + Clone, V> { shards: Vec<HashMap<K, V>>, shard_count: usize }
impl<K: Hash + Eq + Clone, V> ShardMap<K, V> {
    pub fn new(shard_count: usize) -> Self {
        Self { shards: vec![HashMap::new(); shard_count], shard_count: shard_count.max(1) }
    }
    fn idx<Q: Hash>(&self, k: &Q) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        let mut h = DefaultHasher::new();
        k.hash(&mut h);
        (h.finish() as usize) % self.shard_count
    }
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        let idx = self.idx(&k);
        self.shards[idx].insert(k, v)
    }
    pub fn get<Q>(&self, k: &Q) -> Option<&V> where K: std::borrow::Borrow<Q>, Q: Hash + ?Sized {
        let idx = self.idx(k);
        self.shards[idx].get(k)
    }
    pub fn remove<Q>(&mut self, k: &Q) -> Option<V> where K: std::borrow::Borrow<Q>, Q: Hash + ?Sized {
        let idx = self.idx(k);
        self.shards[idx].remove(k)
    }
    pub fn total_len(&self) -> usize { self.shards.iter().map(|s| s.len()).sum() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn insert_get() { let mut m = ShardMap::new(4); m.insert("a", 1); assert_eq!(m.get("a"), Some(&1)); }
    #[test] fn multiple_keys() { let mut m = ShardMap::new(4); m.insert("a", 1); m.insert("b", 2); m.insert("c", 3); assert_eq!(m.total_len(), 3); }
    #[test] fn missing_returns_none() { let m: ShardMap<&str, i32> = ShardMap::new(4); assert_eq!(m.get("missing"), None); }
    #[test] fn remove() { let mut m = ShardMap::new(4); m.insert("a", 1); assert_eq!(m.remove("a"), Some(1)); assert_eq!(m.get("a"), None); }
    #[test] fn shard_count_min_1() { let m: ShardMap<&str, i32> = ShardMap::new(0); assert_eq!(m.shard_count, 1); }
    #[test] fn overwrites() { let mut m = ShardMap::new(4); m.insert("a", 1); m.insert("a", 2); assert_eq!(m.get("a"), Some(&2)); }
}
