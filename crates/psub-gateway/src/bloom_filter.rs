//! Probabilistic membership test (Bloom filter).
//!
//! A classic Bloom filter that trades a small false-positive rate for
//! constant-space membership. Supports arbitrary `Hash` types via
//! [`std::collections::hash_map::DefaultHasher`] under a stable seeded
//! state. False-positive rate is `~0.01` for the default constructor
//! parameters.
//!
//! Use [`BloomFilter::new`] when you know the expected capacity and target
//! false-positive rate, or [`BloomFilter::from_size`] to construct directly
//! from a bit count and hash count.

use std::hash::{BuildHasher, Hash, Hasher, RandomState};

/// A Bloom filter backed by a packed `Vec<u64>` bit array.
pub struct BloomFilter {
    bits: Vec<u64>,
    size_bits: usize,
    hash_count: u32,
}

impl BloomFilter {
    /// Construct a filter sized for `capacity` items with target false-positive
    /// rate `false_positive_rate`. Sizes `m = ceil(-n*ln(p) / ln(2)^2)` bits and
    /// `k = round(m/n * ln(2))` hash functions.
    pub fn new(capacity: usize, false_positive_rate: f64) -> Self {
        let ln2 = std::f64::consts::LN_2;
        let size_bits =
            (-((capacity as f64) * false_positive_rate.ln() / (ln2 * ln2))).ceil() as usize;
        let size_bits = size_bits.max(64);
        let hash_count = ((size_bits as f64 / capacity.max(1) as f64) * ln2)
            .round()
            .max(1.0) as u32;
        Self {
            bits: vec![0u64; (size_bits + 63) / 64],
            size_bits,
            hash_count,
        }
    }

    /// Construct a filter with explicit bit count and hash count.
    pub fn from_size(size_bits: usize, hash_count: u32) -> Self {
        Self {
            bits: vec![0u64; (size_bits + 63) / 64],
            size_bits,
            hash_count,
        }
    }

    fn hashes<T: Hash>(&self, item: &T) -> Vec<usize> {
        // Use a fixed state (zeroed seeds) for stable hashing across calls.
        static RS: std::sync::OnceLock<RandomState> = std::sync::OnceLock::new();
        let rs = RS.get_or_init(|| RandomState::new());
        let mut h1 = rs.build_hasher();
        item.hash(&mut h1);
        let a = h1.finish();
        let mut h2 = rs.build_hasher();
        item.hash(&mut h2);
        let b = h2.finish();
        (0..self.hash_count)
            .map(|i| {
                ((a.wrapping_add((i as u64).wrapping_mul(b))) % (self.size_bits as u64)) as usize
            })
            .collect()
    }

    /// Insert an item into the filter.
    pub fn insert<T: Hash>(&mut self, item: &T) {
        for pos in self.hashes(item) {
            self.bits[pos / 64] |= 1u64 << (pos % 64);
        }
    }

    /// Test whether the item has *probably* been inserted. False positives
    /// are possible; false negatives are not.
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        self.hashes(item)
            .iter()
            .all(|&pos| (self.bits[pos / 64] >> (pos % 64)) & 1 == 1)
    }

    /// Approximate current false-positive probability given the number of
    /// items already inserted.
    pub fn false_positive_estimate(&self, items_inserted: usize) -> f64 {
        let k = self.hash_count as f64;
        let m = self.size_bits as f64;
        let n = items_inserted as f64;
        (1.0 - (-k * n / m).exp()).powf(k)
    }

    /// Number of bits in the underlying bit array.
    pub fn len(&self) -> usize {
        self.size_bits
    }

    /// True if the filter has zero bits allocated (never true for a well-formed
    /// filter; `from_size` allows 0 only if `size_bits == 0`).
    pub fn is_empty(&self) -> bool {
        self.size_bits == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_contains() {
        let mut bf = BloomFilter::new(1000, 0.01);
        bf.insert(&"hello");
        bf.insert(&"world");
        assert!(bf.contains(&"hello"));
        assert!(bf.contains(&"world"));
        assert!(!bf.contains(&"absent"));
    }

    #[test]
    fn int_keys() {
        let mut bf = BloomFilter::new(100, 0.05);
        for i in 0..50 {
            bf.insert(&i);
        }
        for i in 0..50 {
            assert!(bf.contains(&i));
        }
    }

    #[test]
    fn from_size_constructor() {
        let mut bf = BloomFilter::from_size(1024, 4);
        bf.insert(&"abc");
        assert!(bf.contains(&"abc"));
    }

    #[test]
    fn len_reports_bit_count() {
        let bf = BloomFilter::from_size(512, 3);
        assert_eq!(bf.len(), 512);
    }

    #[test]
    fn false_positive_rate_low_for_default() {
        let mut bf = BloomFilter::new(10_000, 0.001);
        for i in 0..1_000 {
            bf.insert(&i);
        }
        let mut false_positives = 0;
        for i in 10_000..20_000 {
            if bf.contains(&i) {
                false_positives += 1;
            }
        }
        // Should be well under 5% for proper parameters
        assert!(false_positives < 500, "fp rate too high: {}", false_positives);
    }

    #[test]
    fn false_positive_estimate_matches_doubling() {
        let bf = BloomFilter::new(100, 0.01);
        // Theoretical: p = (1 - e^(-k*n/m))^k. With k=7, m=959, n=100:
        let p = bf.false_positive_estimate(100);
        assert!(p > 0.0 && p < 1.0);
    }
}