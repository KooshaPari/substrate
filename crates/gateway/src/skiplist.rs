use std::cmp::Ordering;

pub struct SkipList<T: Ord> { head: usize, levels: Vec<Vec<(usize, T)>> }
impl<T: Ord + Clone> SkipList<T> {
    pub fn new(max_levels: usize) -> Self {
        Self { head: 0, levels: (0..max_levels).map(|_| Vec::new()).collect() }
    }
    pub fn insert(&mut self, v: T) {
        let mut level = 0;
        while level < self.levels.len() - 1 && (level * 7 + 3) % 11 > 4 { level += 1; }
        self.levels[level].push((self.head, v));
    }
    pub fn contains(&self, v: &T) -> bool {
        for level in self.levels.iter().rev() {
            for (_, val) in level.iter() {
                match val.cmp(v) {
                    Ordering::Equal => return true,
                    Ordering::Greater => break,
                    Ordering::Less => {}
                }
            }
        }
        false
    }
    pub fn len(&self) -> usize { self.levels.iter().map(|l| l.len()).sum() }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn insert_contains() { let mut s = SkipList::new(4); s.insert(5); s.insert(3); assert!(s.contains(&5)); assert!(s.contains(&3)); }
    #[test] fn missing() { let mut s = SkipList::new(4); s.insert(5); assert!(!s.contains(&3)); }
    #[test] fn len() { let mut s = SkipList::new(4); for i in 0..10 { s.insert(i); } assert_eq!(s.len(), 10); }
}
