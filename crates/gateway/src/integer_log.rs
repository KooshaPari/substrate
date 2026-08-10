//! Integer logarithm helpers (bit-twiddling).
//!
//! Fast floor logarithms for `u32`/`u64` using only bit operations — no
//! floating point, no external dependencies, no lookup tables. Useful in
//! memory allocators (slab index from size), hashing (bucket selection),
//! and any tight loop where a `f64::log2` call would dominate.
//!
//! Reference: "Bit Twiddling Hacks", Stanford Bit Twiddling Hacks collection
//! (Henry S. Warren, "Hacker's Delight", §5-3 / §11-4).

/// Floor log base 2 of `x`. Returns 0 for `x == 1`, the position of the
/// highest set bit for `x >= 2`, and `0` for `x == 0` (by convention).
///
/// # Examples
/// ```
/// use gateway::integer_log::log2_u32;
/// assert_eq!(log2_u32(1), 0);
/// assert_eq!(log2_u32(2), 1);
/// assert_eq!(log2_u32(3), 1);
/// assert_eq!(log2_u32(4), 2);
/// assert_eq!(log2_u32(1024), 10);
/// ```
#[inline]
pub fn log2_u32(x: u32) -> u32 {
    if x == 0 {
        return 0;
    }
    31 - x.leading_zeros()
}

/// Ceiling log base 2 of `x`. Returns the smallest `n` such that `2^n >= x`.
/// Returns 0 for `x <= 1`.
///
/// # Examples
/// ```
/// use gateway::integer_log::ceil_log2_u32;
/// assert_eq!(ceil_log2_u32(1), 0);
/// assert_eq!(ceil_log2_u32(2), 1);
/// assert_eq!(ceil_log2_u32(3), 2);
/// assert_eq!(ceil_log2_u32(4), 2);
/// assert_eq!(ceil_log2_u32(5), 3);
/// ```
#[inline]
pub fn ceil_log2_u32(x: u32) -> u32 {
    if x <= 1 {
        return 0;
    }
    32 - (x - 1).leading_zeros()
}

/// Floor log base 2 of `x` for u64. Returns 0 for `x == 0`.
///
/// # Examples
/// ```
/// use gateway::integer_log::log2_u64;
/// assert_eq!(log2_u64(1), 0);
/// assert_eq!(log2_u64(1024), 10);
/// assert_eq!(log2_u64(1u64 << 63), 63);
/// ```
#[inline]
pub fn log2_u64(x: u64) -> u32 {
    if x == 0 {
        return 0;
    }
    63 - x.leading_zeros()
}

/// Floor log base 10 of `x`. Returns the number of decimal digits minus 1
/// (e.g. 100 -> 2). Returns 0 for `x == 0`.
///
/// # Examples
/// ```
/// use gateway::integer_log::log10_u32;
/// assert_eq!(log10_u32(1), 0);
/// assert_eq!(log10_u32(9), 0);
/// assert_eq!(log10_u32(10), 1);
/// assert_eq!(log10_u32(99), 1);
/// assert_eq!(log10_u32(100), 2);
/// ```
pub fn log10_u32(x: u32) -> u32 {
    if x == 0 {
        return 0;
    }
    let mut log = 0u32;
    let mut v = x;
    // Repeated comparison against powers of 10.
    if v >= 1_000_000_000 {
        log += 9;
        v /= 1_000_000_000;
    }
    if v >= 1_000_000 {
        log += 6;
        v /= 1_000_000;
    }
    if v >= 1_000 {
        log += 3;
        v /= 1_000;
    }
    if v >= 100 {
        log += 2;
    } else if v >= 10 {
        log += 1;
    }
    log
}

/// Number of decimal digits in `x`. Returns 1 for `x == 0`.
///
/// # Examples
/// ```
/// use gateway::integer_log::num_decimal_digits_u32;
/// assert_eq!(num_decimal_digits_u32(0), 1);
/// assert_eq!(num_decimal_digits_u32(9), 1);
/// assert_eq!(num_decimal_digits_u32(10), 2);
/// assert_eq!(num_decimal_digits_u32(999), 3);
/// assert_eq!(num_decimal_digits_u32(1_000_000_000), 10);
/// ```
pub fn num_decimal_digits_u32(x: u32) -> u32 {
    log10_u32(x) + 1
}

/// Largest power of 2 less than or equal to `x`. Returns 0 for `x == 0`.
///
/// # Examples
/// ```
/// use gateway::integer_log::prev_pow2_u32;
/// assert_eq!(prev_pow2_u32(0), 0);
/// assert_eq!(prev_pow2_u32(1), 1);
/// assert_eq!(prev_pow2_u32(2), 2);
/// assert_eq!(prev_pow2_u32(3), 2);
/// assert_eq!(prev_pow2_u32(1024), 1024);
/// assert_eq!(prev_pow2_u32(1025), 1024);
/// ```
#[inline]
pub fn prev_pow2_u32(x: u32) -> u32 {
    if x == 0 {
        return 0;
    }
    1u32 << log2_u32(x)
}

/// Smallest power of 2 greater than or equal to `x`. Returns 1 for `x <= 1`.
///
/// # Examples
/// ```
/// use gateway::integer_log::next_pow2_u32;
/// assert_eq!(next_pow2_u32(0), 1);
/// assert_eq!(next_pow2_u32(1), 1);
/// assert_eq!(next_pow2_u32(2), 2);
/// assert_eq!(next_pow2_u32(3), 4);
/// assert_eq!(next_pow2_u32(1024), 1024);
/// assert_eq!(next_pow2_u32(1025), 2048);
/// ```
#[inline]
pub fn next_pow2_u32(x: u32) -> u32 {
    if x <= 1 {
        return 1;
    }
    1u32 << ceil_log2_u32(x)
}

/// Count the number of set bits (population count / Hamming weight).
/// Equivalent to `x.count_ones()` but exposed under a stable name and
/// retained for documentation symmetry.
#[inline]
pub fn popcount_u32(x: u32) -> u32 {
    x.count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ref_log2(x: u32) -> u32 {
        if x == 0 {
            return 0;
        }
        (x as f64).log2() as u32
    }

    fn ref_ceil_log2(x: u32) -> u32 {
        if x <= 1 {
            return 0;
        }
        let mut n = 0u32;
        let mut v = 1u32;
        while v < x {
            v <<= 1;
            n += 1;
        }
        n
    }

    fn ref_log10(x: u32) -> u32 {
        if x == 0 {
            return 0;
        }
        (x as f64).log10() as u32
    }

    fn ref_num_digits(x: u32) -> u32 {
        if x == 0 {
            return 1;
        }
        let mut n = 0u32;
        let mut v = x;
        while v > 0 {
            v /= 10;
            n += 1;
        }
        n
    }

    #[test]
    fn log2_zero_is_zero() {
        assert_eq!(log2_u32(0), 0);
        assert_eq!(log2_u64(0), 0);
    }

    #[test]
    fn log2_u32_small_values() {
        assert_eq!(log2_u32(1), 0);
        assert_eq!(log2_u32(2), 1);
        assert_eq!(log2_u32(3), 1);
        assert_eq!(log2_u32(4), 2);
        assert_eq!(log2_u32(7), 2);
        assert_eq!(log2_u32(8), 3);
        assert_eq!(log2_u32(1024), 10);
    }

    #[test]
    fn log2_u32_matches_reference() {
        for x in 0..=4096 {
            assert_eq!(log2_u32(x), ref_log2(x), "log2_u32({})", x);
        }
    }

    #[test]
    fn log2_u64_high_bit() {
        assert_eq!(log2_u64(1u64 << 63), 63);
        assert_eq!(log2_u64(u64::MAX), 63);
    }

    #[test]
    fn ceil_log2_u32_matches_reference() {
        for x in 0..=4096 {
            assert_eq!(ceil_log2_u32(x), ref_ceil_log2(x), "ceil_log2({})", x);
        }
    }

    #[test]
    fn ceil_log2_u32_edge_cases() {
        assert_eq!(ceil_log2_u32(0), 0);
        assert_eq!(ceil_log2_u32(1), 0);
        assert_eq!(ceil_log2_u32(2), 1);
        assert_eq!(ceil_log2_u32(3), 2);
        assert_eq!(ceil_log2_u32(4), 2);
    }

    #[test]
    fn log10_u32_matches_reference() {
        for x in 0..=10_000 {
            assert_eq!(log10_u32(x), ref_log10(x), "log10({})", x);
        }
        // Spot-check edge cases above.
        assert_eq!(log10_u32(999_999_999), 8);
        assert_eq!(log10_u32(1_000_000_000), 9);
        assert_eq!(log10_u32(u32::MAX), 9);
    }

    #[test]
    fn num_decimal_digits_u32_matches_reference() {
        for x in 0..=10_000 {
            assert_eq!(
                num_decimal_digits_u32(x),
                ref_num_digits(x),
                "num_digits({})",
                x
            );
        }
    }

    #[test]
    fn prev_pow2_zero_is_zero() {
        assert_eq!(prev_pow2_u32(0), 0);
    }

    #[test]
    fn prev_pow2_powers_of_two() {
        for n in 0..31 {
            let p = 1u32 << n;
            assert_eq!(prev_pow2_u32(p), p, "prev_pow2({})", p);
        }
    }

    #[test]
    fn prev_pow2_between_powers() {
        assert_eq!(prev_pow2_u32(5), 4);
        assert_eq!(prev_pow2_u32(1023), 512);
        assert_eq!(prev_pow2_u32(1025), 1024);
    }

    #[test]
    fn next_pow2_powers_of_two() {
        for n in 0..31 {
            let p = 1u32 << n;
            assert_eq!(next_pow2_u32(p), p, "next_pow2({})", p);
        }
    }

    #[test]
    fn next_pow2_just_above() {
        assert_eq!(next_pow2_u32(0), 1);
        assert_eq!(next_pow2_u32(1), 1);
        assert_eq!(next_pow2_u32(3), 4);
        assert_eq!(next_pow2_u32(1025), 2048);
    }

    #[test]
    fn popcount_matches_intrinsic() {
        assert_eq!(popcount_u32(0), 0);
        assert_eq!(popcount_u32(0b1011), 3);
        assert_eq!(popcount_u32(0xFFFFFFFF), 32);
        assert_eq!(popcount_u32(u32::MAX), 32);
    }

    #[test]
    fn next_pow2_does_not_overflow() {
        // Largest power of 2 <= u32::MAX should still work.
        let p = 1u32 << 30;
        assert_eq!(next_pow2_u32(p + 1), 1u32 << 31);
    }
}
