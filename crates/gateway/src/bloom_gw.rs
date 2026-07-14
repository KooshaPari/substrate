pub struct Bloom {
    bits: Vec<bool>,
    size: usize,
    hashes: u32,
}
impl Bloom {
    pub fn new(size: usize, hashes: u32) -> Self {
        Self {
            bits: vec![false; size],
            size,
            hashes,
        }
    }
    fn h(&self, item: &str, i: u32) -> usize {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in item.bytes() {
            h = h.wrapping_mul(0x100000001b3) ^ b as u64;
        }
        ((h.wrapping_add(i as u64 * 17)) % self.size as u64) as usize
    }
    pub fn insert(&mut self, item: &str) {
        for i in 0..self.hashes {
            let h = self.h(item, i);
            self.bits[h] = true;
        }
    }
    pub fn may_contain(&self, item: &str) -> bool {
        (0..self.hashes).all(|i| self.bits[self.h(item, i)])
    }
    pub fn false_positive_rate(&self) -> f64 {
        let m = self.bits.iter().filter(|&&b| b).count() as f64;
        m / self.size as f64
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_insert_may_contain() {
        let mut b = Bloom::new(256, 3);
        b.insert("hello");
        assert!(b.may_contain("hello"));
    }
    #[test]
    fn test_different_keys() {
        let mut b = Bloom::new(1024, 4);
        b.insert("alpha");
        assert!(!b.may_contain("beta"));
    }
    #[test]
    fn test_fp_rate() {
        let b = Bloom::new(100, 3);
        assert!(b.false_positive_rate() < 0.05);
    }
}
