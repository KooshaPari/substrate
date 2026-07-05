pub struct BitSet { words: Vec<u64> }
impl BitSet {
    pub fn new(capacity_bits: usize) -> Self { let n_words = (capacity_bits + 63) / 64; Self { words: vec![0; n_words] } }
    pub fn set(&mut self, idx: usize) { let (w, b) = (idx / 64, idx % 64); if w < self.words.len() { self.words[w] |= 1u64 << b; } }
    pub fn clear(&mut self, idx: usize) { let (w, b) = (idx / 64, idx % 64); if w < self.words.len() { self.words[w] &= !(1u64 << b); } }
    pub fn get(&self, idx: usize) -> bool { let (w, b) = (idx / 64, idx % 64); w < self.words.len() && self.words[w] & (1u64 << b) != 0 }
    pub fn count(&self) -> u32 { self.words.iter().map(|w| w.count_ones()).sum() }
    pub fn union(&mut self, other: &BitSet) { for (a, b) in self.words.iter_mut().zip(other.words.iter()) { *a |= *b; } }
    pub fn intersect(&mut self, other: &BitSet) { for (a, b) in self.words.iter_mut().zip(other.words.iter()) { *a &= *b; } }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn set_get() { let mut b = BitSet::new(64); b.set(5); assert!(b.get(5)); assert!(!b.get(4)); }
    #[test] fn clear() { let mut b = BitSet::new(64); b.set(5); b.clear(5); assert!(!b.get(5)); }
    #[test] fn count() { let mut b = BitSet::new(64); b.set(0); b.set(63); assert_eq!(b.count(), 2); }
    #[test] fn union() { let mut a = BitSet::new(64); let mut b = BitSet::new(64); a.set(1); b.set(2); a.union(&b); assert!(a.get(1) && a.get(2)); }
    #[test] fn intersect() { let mut a = BitSet::new(64); let mut b = BitSet::new(64); a.set(1); a.set(2); b.set(2); a.intersect(&b); assert!(!a.get(1) && a.get(2)); }
}
