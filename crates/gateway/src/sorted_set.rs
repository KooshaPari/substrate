pub struct SortedSet<T: Ord + Clone> { items: Vec<T> }
impl<T: Ord + Clone> SortedSet<T> {
    pub fn new() -> Self { Self { items: Vec::new() } }
    pub fn insert(&mut self, v: T) -> bool {
        match self.items.binary_search(&v) {
            Ok(_) => false,
            Err(pos) => { self.items.insert(pos, v); true }
        }
    }
    pub fn remove(&mut self, v: &T) -> bool {
        match self.items.binary_search(v) {
            Ok(pos) => { self.items.remove(pos); true }
            Err(_) => false
        }
    }
    pub fn contains(&self, v: &T) -> bool { self.items.binary_search(v).is_ok() }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    pub fn iter(&self) -> std::slice::Iter<T> { self.items.iter() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn insert_ordered() { let mut s = SortedSet::new(); s.insert(3); s.insert(1); s.insert(2); assert_eq!(s.iter().copied().collect::<Vec<_>>(), vec![1,2,3]); }
    #[test] fn dedup() { let mut s = SortedSet::new(); assert!(s.insert(1)); assert!(!s.insert(1)); }
    #[test] fn contains() { let mut s = SortedSet::new(); s.insert(5); assert!(s.contains(&5)); assert!(!s.contains(&3)); }
    #[test] fn remove() { let mut s = SortedSet::new(); s.insert(1); assert!(s.remove(&1)); assert!(!s.remove(&1)); }
    #[test] fn empty() { let s: SortedSet<i32> = SortedSet::new(); assert!(s.is_empty()); }
}
