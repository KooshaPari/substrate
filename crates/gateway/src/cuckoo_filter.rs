//! Cuckoo filter — probabilistic set membership with deletion.
//!
//! Like a Bloom filter, but supports deletions and uses less space per
//! item. Items are stored as fingerprints (8 or 16 bit) in two candidate
//! buckets; the kick-out chain relocates existing items on collision.
//!
//! False-positive rate: roughly `2 * b / 2^f` where `b` is the bucket
//! size and `f` is the fingerprint size in bits. With 8-bit fingerprints
//! and bucket size 4, ~0.04% at 95% load.
//!
//! Reference: Fan, Andersen, Kaminsky, Mitzenmacher, "Cuckoo Filter:
//! Practically Better Than Bloom" (CoNEXT 2014).

use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

const FINGERPRINT_BITS: u32 = 8;
const FINGERPRINT_MASK: u64 = (1 << FINGERPRINT_BITS) - 1;
const MAX_KICKS: usize = 500;

/// A cuckoo filter for set membership tests with deletion support.
pub struct CuckooFilter {
    /// Buckets, each holding up to `BUCKET_SIZE` fingerprints.
    buckets: Vec<Vec<u8>>,
    /// Cached num_buckets for modular arithmetic.
    num_buckets: usize,
    /// Slot count per bucket.
    bucket_size: usize,
    /// Capacity the filter was sized for (in items).
    capacity: usize,
}

impl CuckooFilter {
    /// Create a new cuckoo filter sized for `capacity` items, using
    /// 4 fingerprints per bucket. Round up `capacity` to a power of two
    /// for cheap bitmask indexing.
    pub fn new(capacity: usize) -> Self {
        let num_buckets = (capacity.max(1)).next_power_of_two();
        Self::with_bucket_size(capacity, 4)
            .into_num_buckets(num_buckets)
    }

    fn with_bucket_size(capacity: usize, bucket_size: usize) -> Self {
        let num_buckets = (capacity.max(1) / bucket_size.max(1)).next_power_of_two();
        Self {
            buckets: vec![Vec::with_capacity(bucket_size); num_buckets],
            num_buckets,
            bucket_size,
            capacity,
        }
    }

    fn into_num_buckets(mut self, n: usize) -> Self {
        self.buckets = vec![Vec::with_capacity(self.bucket_size); n];
        self.num_buckets = n;
        self
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.buckets.iter().map(|b| b.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the filter is at risk of insertion failure.
    pub fn is_full(&self) -> bool {
        // Each bucket can hold `bucket_size` items.
        self.buckets.iter().all(|b| b.len() >= self.bucket_size)
    }

    /// Insert a value. Returns `Ok(())` on success, `Err(value)` if
    /// the filter is too full to accommodate after MAX_KICKS relocations.
    pub fn insert<T: Hash>(&mut self, value: &T) -> Result<(), String> {
        let fp = fingerprint(value);
        if fp == 0 {
            return Err("zero fingerprint — would be ambiguous with empty slot".into());
        }
        let (i1, i2) = self.indices(value);

        if self.buckets[i1].len() < self.bucket_size {
            self.buckets[i1].push(fp);
            return Ok(());
        }
        if self.buckets[i2].len() < self.bucket_size {
            self.buckets[i2].push(fp);
            return Ok(());
        }

        // Both buckets full — kick a random entry from one of them and
        // try to relocate it in its alternate bucket.
        let mut rng_state = simple_rng();
        let mut idx = if rng_state.next() & 1 == 0 { i1 } else { i2 };
        let mut fp = fp;

        for _ in 0..MAX_KICKS {
            let bucket = &mut self.buckets[idx];
            let victim = rng_state.next() % bucket.len();
            let evicted_fp = bucket[victim];
            bucket[victim] = fp;
            fp = evicted_fp;

            // Compute the alternate bucket for the evicted fingerprint.
            let fp_hash = fingerprint_to_index(fp);
            let alt_idx = idx ^ fp_hash;
            if alt_idx >= self.num_buckets {
                // alt_idx could overflow; mask to num_buckets (power of two).
                let alt_idx = alt_idx & (self.num_buckets - 1);
                if self.buckets[alt_idx].len() < self.bucket_size {
                    self.buckets[alt_idx].push(fp);
                    return Ok(());
                }
                idx = alt_idx;
            } else {
                if self.buckets[alt_idx].len() < self.bucket_size {
                    self.buckets[alt_idx].push(fp);
                    return Ok(());
                }
                idx = alt_idx;
            }
        }
        Err(format!("cuckoo filter full after {} kicks", MAX_KICKS))
    }

    /// Returns true if `value` might be in the set. False positives
    /// are possible; false negatives are not.
    pub fn contains<T: Hash>(&self, value: &T) -> bool {
        let fp = fingerprint(value);
        if fp == 0 {
            // Treat zero-fingerprint values as "not present" — we
            // rejected them on insert, so they couldn't be in the set.
            return false;
        }
        let (i1, i2) = self.indices(value);
        self.buckets[i1].contains(&fp) || self.buckets[i2].contains(&fp)
    }

    /// Delete a previously-inserted value. Returns true if the value
    /// was found and removed; false if it wasn't (either never inserted
    /// or already deleted).
    pub fn delete<T: Hash>(&mut self, value: &T) -> bool {
        let fp = fingerprint(value);
        if fp == 0 {
            return false;
        }
        let (i1, i2) = self.indices(value);
        if let Some(pos) = self.buckets[i1].iter().position(|&x| x == fp) {
            self.buckets[i1].swap_remove(pos);
            return true;
        }
        if let Some(pos) = self.buckets[i2].iter().position(|&x| x == fp) {
            self.buckets[i2].swap_remove(pos);
            return true;
        }
        false
    }

    fn indices<T: Hash>(&self, value: &T) -> (usize, usize) {
        let h = hash_value(value);
        let i1 = (h as usize) & (self.num_buckets - 1);
        let fp = fingerprint(value);
        let fp_hash = fingerprint_to_index(fp);
        let i2 = (i1 ^ (fp_hash as usize)) & (self.num_buckets - 1);
        (i1, i2)
    }
}

fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

fn fingerprint<T: Hash>(value: &T) -> u8 {
    let h = hash_value(value);
    let fp = (h >> 32) & FINGERPRINT_MASK;
    fp as u8
}

fn fingerprint_to_index(fp: u8) -> usize {
    // Mix fingerprint bits into an index offset. The cuckoo filter
    // paper recommends using the hash of the fingerprint itself to keep
    // the two indices independent.
    let mut h: u64 = fp as u64;
    h = h.wrapping_mul(0x5bd1e9955bd1e995).wrapping_add(0xe6546b64);
    (h as usize) & 0xFFFF
}

/// Tiny, fast, deterministic PRNG (xorshift64*) for picking kick
/// victims. Not cryptographic; only used for the kick-out chain.
struct SimpleRng(u64);

impl SimpleRng {
    fn next(&mut self) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        self.0 as usize
    }
}

fn simple_rng() -> SimpleRng {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEADBEEF);
    SimpleRng(nanos | 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_contains() {
        let mut f = CuckooFilter::new(64);
        f.insert(&"hello").unwrap();
        f.insert(&"world").unwrap();
        assert!(f.contains(&"hello"));
        assert!(f.contains(&"world"));
        assert!(!f.contains(&"missing"));
    }

    #[test]
    fn delete_removes_value() {
        let mut f = CuckooFilter::new(64);
        f.insert(&"hello").unwrap();
        assert!(f.contains(&"hello"));
        assert!(f.delete(&"hello"));
        assert!(!f.contains(&"hello"));
    }

    #[test]
    fn delete_missing_returns_false() {
        let mut f = CuckooFilter::new(64);
        assert!(!f.delete(&"missing"));
    }

    #[test]
    fn delete_then_reinsert() {
        let mut f = CuckooFilter::new(64);
        f.insert(&"hello").unwrap();
        f.delete(&"hello");
        f.insert(&"hello").unwrap();
        assert!(f.contains(&"hello"));
    }

    #[test]
    fn handles_numeric_values() {
        let mut f = CuckooFilter::new(64);
        for i in 0..100 {
            f.insert(&i).unwrap();
        }
        for i in 0..100 {
            assert!(f.contains(&i), "should contain {}", i);
        }
    }

    #[test]
    fn saturation_at_capacity() {
        // Test that load factor is bounded; some inserts at high load
        // may fail with Err, but no panic.
        let mut f = CuckooFilter::new(16);
        let mut ok = 0;
        for i in 0..200 {
            if f.insert(&i).is_ok() {
                ok += 1;
            }
        }
        assert!(ok > 16, "expected to insert at least capacity items");
        // The filter should not panic; len() must be <= num_buckets * bucket_size.
        assert!(f.len() <= f.num_buckets * f.bucket_size);
    }

    #[test]
    fn capacity_zero_works() {
        // Capacity 0 should still create a small filter (1 bucket).
        let f = CuckooFilter::new(0);
        assert_eq!(f.capacity(), 0);
        assert!(f.is_empty());
    }
}