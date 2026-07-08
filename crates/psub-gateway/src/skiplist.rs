pub struct SkipList<T: Ord> { levels: Vec<Vec<T>> }
impl<T: Ord + Clone> SkipList<T> {
    pub fn new(max_levels: usize) -> Self { Self { levels: (0..max_levels).map(|_| Vec::new()).collect() } }
    pub fn insert(&mut self, v: T) { for level in self.levels.iter_mut() { level.push(v.clone()); } }
    pub fn contains(&self, v: &T) -> bool { self.levels.iter().any(|l| l.iter().any(|x| x == v)) }
    pub fn len(&self) -> usize { self.levels.iter().map(|l| l.len()).sum() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn insert_contains() { let mut s = SkipList::new(4); s.insert(5); s.insert(3); assert!(s.contains(&5)); assert!(s.contains(&3)); }
    #[test] fn missing() { let mut s = SkipList::new(4); s.insert(5); assert!(!s.contains(&3)); }
    #[test] fn len() { let mut s = SkipList::new(4); for i in 0..10 { s.insert(i); } assert_eq!(s.len(), 40); }
}
