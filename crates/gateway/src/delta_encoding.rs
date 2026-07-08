//! Delta, ZigZag, and unsigned LEB128 (varint) encoding.
//!
//! Three small integer codecs commonly paired together when serialising
//! mostly-monotonic numeric sequences (timestamps, sorted IDs, byte
//! offsets):
//!
//! * [`delta_i64`] / [`undelta_i64`] — encode successive integers as their
//!   signed difference (`v[i] - v[i-1]`); the first value is emitted as-is.
//! * [`zigzag_encode`] / [`zigzag_decode`] — map signed integers to
//!   unsigned so that small magnitudes (positive *or* negative) get small
//!   two's-complement codings. Defined as `zz(n) = (n << 1) ^ (n >> 63)`
//!   for i64, the canonical formula also used by protobuf.
//! * [`uleb128_encode`] / [`uleb128_decode`] — variable-length unsigned
//!   integer encoding used by DWARF, Protocol Buffers, WebAssembly, and
//!   many binary file formats. Each byte carries 7 payload bits; the high
//!   bit is the continuation flag. Maximum length is 10 bytes for u64.
//!
//! None of these functions allocate on the encoding hot path beyond the
//! returned `Vec<u8>`. The decoders accept any slice (no length check)
//! and surface truncated or malformed inputs as `Err`.

/// Encode a sequence of signed integers as first-value + signed deltas.
///
/// Empty input yields an empty output. The first output element is the
/// first input value (verbatim); subsequent outputs are
/// `input[i] - input[i-1]`.
pub fn delta_i64(input: &[i64]) -> Vec<i64> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(input.len());
    out.push(input[0]);
    for w in input.windows(2) {
        out.push(w[1].wrapping_sub(w[0]));
    }
    out
}

/// Reverse of [`delta_i64`]. Empty input is the empty output. A single
/// element is returned verbatim.
pub fn undelta_i64(input: &[i64]) -> Vec<i64> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(input.len());
    let mut acc = input[0];
    out.push(acc);
    for &d in &input[1..] {
        acc = acc.wrapping_add(d);
        out.push(acc);
    }
    out
}

/// ZigZag encode an `i64` to `u64`. Maps signed integers to unsigned such
/// that small magnitudes — positive *or* negative — get small codes.
///
/// Identity: `zigzag_decode(zigzag_encode(n)) == n` for all `i64` values
/// (including `i64::MIN`).
#[inline]
pub fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

/// ZigZag decode a `u64` to `i64`.
#[inline]
pub fn zigzag_decode(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// LEB128 encode a `u64` into a `Vec<u8>`. Maximum length: 10 bytes.
pub fn uleb128_encode(mut value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        } else {
            out.push(byte | 0x80);
        }
    }
    out
}

/// LEB128 decode a `&[u8]` into a `u64`. Returns `Err` if the input is
/// empty, exceeds 10 bytes (overflows `u64`), or terminates without the
/// high-bit clear.
pub fn uleb128_decode(bytes: &[u8]) -> Result<(u64, usize), &'static str> {
    if bytes.is_empty() {
        return Err("uleb128: empty input");
    }
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut consumed: usize = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if i >= 10 {
            return Err("uleb128: input exceeds 10 bytes (u64 overflow)");
        }
        let payload = (b & 0x7F) as u64;
        let cont = b & 0x80 != 0;
        if shift >= 64 {
            return Err("uleb128: u64 overflow");
        }
        // The maximum allowed shift that does not lose bits is 63
        // (which uses all 64 bits of the result); shift == 56..=63 must
        // not have payload bits beyond the result's high bit.
        if shift == 63 {
            if payload > 1 {
                return Err("uleb128: u64 overflow");
            }
        } else if shift > 63 {
            return Err("uleb128: u64 overflow");
        }
        result |= payload << shift;
        consumed = i + 1;
        if !cont {
            return Ok((result, consumed));
        }
        shift += 7;
    }
    Err("uleb128: truncated input (continuation bit set on last byte)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_empty() {
        assert_eq!(delta_i64(&[]), Vec::<i64>::new());
        assert_eq!(undelta_i64(&[]), Vec::<i64>::new());
    }

    #[test]
    fn delta_single_element() {
        assert_eq!(delta_i64(&[42]), vec![42]);
        assert_eq!(undelta_i64(&[42]), vec![42]);
    }

    #[test]
    fn delta_roundtrip_monotonic() {
        let values: Vec<i64> = (0..100).map(|i| i * 7).collect();
        let d = delta_i64(&values);
        // First element equals first input.
        assert_eq!(d[0], 0);
        // All subsequent deltas equal 7.
        for x in &d[1..] {
            assert_eq!(*x, 7);
        }
        // Round-trip recovers the original.
        assert_eq!(undelta_i64(&d), values);
    }

    #[test]
    fn delta_roundtrip_with_negatives() {
        let values: Vec<i64> = vec![100, 90, 85, 70, 70, 50, 25, 0, -10, -25];
        let d = delta_i64(&values);
        assert_eq!(d, vec![100, -10, -5, -15, 0, -20, -25, -25, -10, -15]);
        assert_eq!(undelta_i64(&d), values);
    }

    #[test]
    fn zigzag_zero() {
        assert_eq!(zigzag_encode(0), 0);
        assert_eq!(zigzag_decode(0), 0);
    }

    #[test]
    fn zigzag_positive_one() {
        assert_eq!(zigzag_encode(1), 2);
        assert_eq!(zigzag_decode(2), 1);
    }

    #[test]
    fn zigzag_negative_one() {
        assert_eq!(zigzag_encode(-1), 1);
        assert_eq!(zigzag_decode(1), -1);
    }

    #[test]
    fn zigzag_roundtrip_full_range() {
        // Spot-check across the full i64 range, including MIN/MAX.
        let cases: &[i64] = &[
            0, 1, -1, 2, -2, 3, -3, 100, -100, 1000, -1000,
            i64::MAX, i64::MIN, i64::MIN + 1, i64::MAX - 1,
        ];
        for &n in cases {
            assert_eq!(zigzag_decode(zigzag_encode(n)), n,
                "zigzag round-trip failed for n={}", n);
        }
    }

    #[test]
    fn zigzag_isomorphism_table() {
        // zz(0) = 0; for k >= 1: zz(k) = 2k, zz(-k) = 2k - 1.
        assert_eq!(zigzag_encode(0), 0);
        for k in 1..100i64 {
            assert_eq!(zigzag_encode(k), (k as u64) * 2);
            assert_eq!(zigzag_encode(-k), (k as u64) * 2 - 1);
        }
    }

    #[test]
    fn uleb128_encode_zero() {
        assert_eq!(uleb128_encode(0), vec![0]);
    }

    #[test]
    fn uleb128_encode_small() {
        // 0..128 fit in one byte.
        for v in 0u64..128 {
            let bytes = uleb128_encode(v);
            assert_eq!(bytes.len(), 1, "v={}", v);
            assert_eq!(bytes[0] as u64, v);
        }
    }

    #[test]
    fn uleb128_encode_two_bytes() {
        // 128..16384 fit in two bytes.
        for v in [128u64, 200, 16_383, 16_384 - 1] {
            let bytes = uleb128_encode(v);
            assert_eq!(bytes.len(), 2, "v={}", v);
            assert_eq!(bytes[0] & 0x80, 0x80, "continuation bit set on byte 0");
            assert_eq!(bytes[1] & 0x80, 0, "no continuation on byte 1");
        }
    }

    #[test]
    fn uleb128_decode_zero() {
        let (v, n) = uleb128_decode(&[0]).unwrap();
        assert_eq!(v, 0);
        assert_eq!(n, 1);
    }

    #[test]
    fn uleb128_decode_two_byte_value() {
        // 200 = 0xC8 0x01 (200 doesn't fit in 7 bits so it needs two bytes).
        let (v, n) = uleb128_decode(&[0xC8, 0x01]).unwrap();
        assert_eq!(v, 200);
        assert_eq!(n, 2);
        // 300 = 0xAC 0x02.
        let (v, n) = uleb128_decode(&[0xAC, 0x02]).unwrap();
        assert_eq!(v, 300);
        assert_eq!(n, 2);
    }

    #[test]
    fn uleb128_roundtrip_full_range() {
        let cases: &[u64] = &[
            0, 1, 127, 128, 16_383, 16_384, (1 << 21) - 1, 1 << 21,
            (1 << 28) - 1, 1 << 28, (1 << 35) - 1, 1 << 35,
            u64::MAX, u64::MAX - 1,
        ];
        for &v in cases {
            let bytes = uleb128_encode(v);
            let (back, n) = uleb128_decode(&bytes).unwrap();
            assert_eq!(back, v, "round-trip failed for v={}", v);
            assert_eq!(n, bytes.len(), "consumed length mismatch for v={}", v);
        }
    }

    #[test]
    fn uleb128_decode_truncated() {
        // Continuation bit set on last byte: error.
        assert!(uleb128_decode(&[0x80]).is_err());
    }

    #[test]
    fn uleb128_decode_empty() {
        assert!(uleb128_decode(&[]).is_err());
    }

    #[test]
    fn uleb128_decode_10_byte_max() {
        // Encode u64::MAX must be exactly 10 bytes.
        let bytes = uleb128_encode(u64::MAX);
        assert_eq!(bytes.len(), 10);
        // Round-trip succeeds.
        let (v, n) = uleb128_decode(&bytes).unwrap();
        assert_eq!(v, u64::MAX);
        assert_eq!(n, 10);
    }

    #[test]
    fn uleb128_decode_overflow() {
        // 11 bytes with continuation on byte 10 — must reject.
        let bytes = vec![0x80u8; 11];
        assert!(uleb128_decode(&bytes).is_err());
    }
}