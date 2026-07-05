use std::collections::HashMap;

pub struct LruCache<K: Clone + std::hash::Hash + Eq, V> {
    map: HashMap<K, V>,
    order: Vec<K>,
    capacity: usize,
}
impl<K: Clone + std::hash::Hash + Eq, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self { Self { map: HashMap::new(), order: Vec::new(), capacity } }
    pub fn put(&mut self, k: K, v: V) {
        if self.map.contains_key(&k) {
            self.map.insert(k.clone(), v);
            self.order.retain(|x| x != &k);
            self.order.push(k);
        } else {
            if self.order.len() >= self.capacity {
                if let Some(evicted) = self.order.first().cloned() {
                    self.order.remove(0);
                    self.map.remove(&evicted);
                }
            }
            self.map.insert(k.clone(), v);
            self.order.push(k);
        }
    }
    pub fn get(&mut self, k: &K) -> Option<&V> {
        if self.map.contains_key(k) {
            self.order.retain(|x| x != k);
            self.order.push(k.clone());
            self.map.get(k)
        } else { None }
    }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn capacity(&self) -> usize { self.capacity }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn put_and_get() { let mut c = LruCache::new(3); c.put("a", 1); assert_eq!(c.get(&"a"), Some(&1)); }
    #[test] fn evicts_oldest() { let mut c = LruCache::new(2); c.put("a",1); c.put("b",2); c.put("c",3); assert_eq!(c.get(&"a"), None); assert_eq!(c.len(), 2); }
    #[test] fn get_refreshes_order() { let mut c = LruCache::new(2); c.put("a",1); c.put("b",2); c.get(&"a"); c.put("c",3); assert_eq!(c.get(&"b"), None); }
    #[test] fn update_existing() { let mut c = LruCache::new(3); c.put("a", 1); c.put("a", 2); assert_eq!(c.get(&"a"), Some(&2)); }
    #[test] fn capacity_bounded() { let mut c = LruCache::new(2); for i in 0..10 { c.put(format!("k{}", i), i); } assert_eq!(c.len(), 2); }
}
