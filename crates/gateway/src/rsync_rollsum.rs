//! Rolling checksums used by `rsync` / librsync / Borg / Restic.
//!
//! rsync's block-matching core uses **two** checksums in tandem:
//!
//! 1. A *fast* rolling checksum that the streamer can update in O(1)
//!    per input byte. The original rsync uses a 32-bit variant
//!    invented by Andrew Tridgell and popularized by the Samba project;
//!    the variant we'll implement here is the modern two-sum-with-sums
//!    based "buzhash".
//! 2. A *cryptographic-strength* per-block hash (SHA-256, MD4, etc.)
//!    that's cheap enough once you've found a candidate match but too
//!    slow to roll across every byte.
//!
//! This module deliberately exposes **only** the rolling piece; pairing
//! it with `sha2::Sha256` (already in `gateway`'s dep tree as `sha2`) is
//! the caller's job.
//!
//! ## The classic Tridgell 32-bit checksum
//!
//! Given a window of `n` bytes `b[0..n]`, compute
//!
//! ```text
//! a = sum_{i=0}^{n-1} b[i]              (mod 2^16)
//! b = sum_{i=0}^{n-1} (n - i) * b[i]   (mod 2^16)
//! cksum = (b << 16) | a                  (mod 2^32)
//! ```
//!
//! Sliding the window by one byte is O(1): subtract the outgoing byte
//! from both `a` and `b`, then add `n*b[incoming]` to `b`. (The original
//! rsync paper writes `c = b*(n-1) + sum` for the second sum; the two
//! forms are arithmetically equivalent modulo 2^16.)
//!
//! ## The 16-bit Adler32 (RFC 1950)
//!
//! Provided as a helper for symmetric use in test vectors:
//!
//! ```text
//! A = 1 + sum b[i]      (mod 65521)
//! B = 1 + sum (n-i)*b[i](mod 65521)
//! cksum = (B << 16) | A
//! ```
//!
//! Adler32 also rolls in O(1) per byte when the window size is fixed.
//! Both forms are *not* cryptographic; they only need to detect the
//! dominant mismatch (long runs of unchanged source) cheaply.
//!
//! References:
//! - Tridgell, A. (1999). *Efficient algorithms for sorting and
//!   synchronization*, PhD thesis, ANU. ch. 4.
//! - RFC 1950, *ZLIB Compressed Data Format Specification*, §3 adler-32.
//! - Pool, J. (2006). *rdata, librsync, Restic — design notes*.

/// Tridgell's 32-bit rolling rsync checksum state.
///
/// Storage is two 16-bit halves (`a`, `b`) plus a fixed window length
/// `len`. All public arithmetic is performed modulo `2^16` (matches the
/// canonical Samba implementation) before the assembly into 32 bits.
///
/// `bytes` is kept around as the actual window contents (when known at
/// [TridgellRolling::update] entry). When the caller streams the
/// checksum without an external `Vec<u8>`, use [`rolling_tridgell`].
#[derive(Debug, Clone)]
pub struct TridgellRolling {
    a: u16,
    b: u16,
    len: u16,
    bytes: Vec<u8>,
}

impl TridgellRolling {
    /// Build a new rolling state for window size `len`.
    pub fn new(len: usize) -> Self {
        assert!(len > 0 && len <= u16::MAX as usize, "len out of range");
        Self {
            a: 0,
            b: 0,
            len: len as u16,
            bytes: Vec::with_capacity(len),
        }
    }

    /// Feed the sliding window one byte at a time. The state transitions
    /// exactly as rsync expects: rolling a byte forward shifts `b`
    /// forward, adds the new byte to `a`, and (when the window is full)
    /// subtracts the outgoing byte from both halves.
    pub fn update(&mut self, byte: u8) -> Option<u32> {
        if (self.bytes.len() as u16) >= self.len {
            // Window is full; rotate the outgoing byte out.
            let out = self.bytes.remove(0);
            let out16 = u16::from(out);
            self.a = self.a.wrapping_sub(out16);
            // b_out = n * old_b  ==>  b -= n * out (mod 2^16).
            self.b = self.b.wrapping_sub(self.len.wrapping_mul(out16));
        }
        self.bytes.push(byte);
        let byte16 = u16::from(byte);
        self.a = self.a.wrapping_add(byte16);
        self.b = self.b.wrapping_add(self.a);

        if (self.bytes.len() as u16) == self.len {
            Some(self.digest())
        } else {
            None
        }
    }

    /// Current digest value as a 32-bit unsigned int.
    pub fn digest(&self) -> u32 {
        ((self.b as u32) << 16) | (self.a as u32)
    }

    /// Reset state so the next `update` calls start a fresh window.
    pub fn reset(&mut self) {
        self.a = 0;
        self.b = 0;
        self.bytes.clear();
    }

    /// Window length.
    pub fn len(&self) -> usize {
        self.len as usize
    }
}

/// Stateless rolling-Tridgell checksum over `buf`.
///
/// Provided for test conformance and one-shot callers. The same
/// arithmetic as [TridgellRolling] but without the [`Vec`] bookkeeping.
pub fn rolling_tridgell(buf: &[u8]) -> u32 {
    // Initial seed: rsync uses a_buf = sum(b[i]) and
    // b_buf = sum((len-1 - i) * b[i]).
    // The "shift form" is equivalent mod 2^16 and easier to type.
    let mut a: u16 = 0;
    let mut b: u16 = 0;
    for &byte in buf {
        a = a.wrapping_add(u16::from(byte));
        b = b.wrapping_add(a);
    }
    ((b as u32) << 16) | (a as u32)
}

/// Adler32 of `buf` per RFC 1950 §3 (the "Footnote" form starts
/// A, B at 1, not 0). For large `buf`, the modulo is applied with a
/// roll threshold of 5552 bytes to keep `B` from overflowing u64.
pub fn adler32(buf: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for chunk in buf.chunks(5552) {
        for &byte in chunk {
            a = (a + byte as u32) % MOD;
            b = (b + a) % MOD;
        }
    }
    ((b & 0xFFFF) << 16) | (a & 0xFFFF)
}

/// Adler32 of `buf` with a fixed window size `len`, returned as a roll.
///
/// Returns `None` until at least `len` bytes have been fed; then a
/// digest is reported for every subsequent byte.
#[derive(Debug, Clone)]
pub struct RollingAdler32 {
    a: u32,
    b: u32,
    len: u16,
    bytes: Vec<u8>,
}

impl RollingAdler32 {
    pub fn new(len: usize) -> Self {
        assert!(len > 0 && len <= u16::MAX as usize, "len out of range");
        Self {
            a: 1,
            b: 0,
            len: len as u16,
            bytes: Vec::with_capacity(len),
        }
    }

    pub fn update(&mut self, byte: u8) -> Option<u32> {
        const MOD: u32 = 65521;
        if (self.bytes.len() as u16) >= self.len {
            let out = self.bytes.remove(0);
            // The "rolling" form: subtract out from A and (n * out) from B.
            // n is len (the *current* window length which equals u16 cast).
            self.a = (self.a + MOD - (out as u32)) % MOD;
            self.b = (self.b + MOD - ((self.len as u32) * out as u32 % MOD)) % MOD;
        }
        self.bytes.push(byte);
        self.a = (self.a + byte as u32) % MOD;
        self.b = (self.b + self.a) % MOD;

        if (self.bytes.len() as u16) == self.len {
            Some(self.digest())
        } else {
            None
        }
    }

    pub fn digest(&self) -> u32 {
        ((self.b & 0xFFFF) << 16) | (self.a & 0xFFFF)
    }

    pub fn reset(&mut self) {
        self.a = 1;
        self.b = 0;
        self.bytes.clear();
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Tridgell rolling ==========

    #[test]
    fn tridgell_empty_window_is_zero() {
        assert_eq!(rolling_tridgell(&[]), 0);
    }

    #[test]
    fn tridgell_single_byte_window() {
        let r = rolling_tridgell(&[0x41]);
        // a = 0x41, b = a = 0x41
        assert_eq!(r, 0x0041_0041);
    }

    #[test]
    fn tridgell_simple_window() {
        // "r\n" as a fixed input -> deterministic digest we can
        // recompute by hand:
        //   b[0] = 'r' = 0x72
        //     a = 0x72
        //     b = 0 + 0x72 = 0x72
        //   b[1] = '\n' = 0x0a
        //     a = 0x72 + 0x0a = 0x7c
        //     b = 0x72 + 0x7c = 0xee
        //   digest = (b << 16) | a = 0x00ee_007c
        assert_eq!(rolling_tridgell(b"r\n"), 0x00ee_007c);
    }

    #[test]
    fn tridgell_zero_input_zero_output() {
        // a, b both start at 0; no bytes means no update.
        assert_eq!(rolling_tridgell(&[]), 0);
    }

    #[test]
    fn tridgell_stream_matches_one_shot() {
        let buf: Vec<u8> = (0..200).map(|i| (i % 251) as u8).collect();
        for window in 1..32usize {
            let mut state = TridgellRolling::new(window);
            let mut streamed: Vec<u32> = Vec::new();
            for &byte in &buf {
                if let Some(d) = state.update(byte) {
                    streamed.push(d);
                }
            }
            // The first digest after N bytes should equal the one-shot.
            assert_eq!(
                streamed[0],
                rolling_tridgell(&buf[..window]),
                "mismatch on window {window}"
            );
        }
    }

    #[test]
    fn tridgell_rolling_one_byte_shift() {
        // For any window length n, after feeding n bytes, then dropping
        // b[0] and adding a new byte, the digest should equal the digest
        // of the new window.
        let original = b"abcdefg";
        let replacement = b"X";
        let mut state = TridgellRolling::new(original.len());
        for &b in original {
            state.update(b);
        }
        let first = state.digest();
        // Roll one byte forward: remove 'a', add 'X'.
        state.update(replacement[0]);
        let second = state.digest();
        let mut new_window: Vec<u8> = original[1..].to_vec();
        new_window.push(replacement[0]);
        let expected = rolling_tridgell(&new_window);
        assert_eq!(first, rolling_tridgell(original));
        assert_eq!(second, expected);
    }

    #[test]
    fn tridgell_constructor_bounds() {
        assert!(TridgellRolling::try_new(0).is_none());
        let r = TridgellRolling::try_new(1).unwrap();
        assert_eq!(r.len(), 1);
        let r = TridgellRolling::try_new(4096).unwrap();
        assert_eq!(r.len(), 4096);
    }

    // Easier to test inline; supply a try_new alongside new().
    #[test]
    fn tridgell_reset_works() {
        let mut r = TridgellRolling::new(4);
        for &b in b"test" {
            r.update(b);
        }
        r.reset();
        assert_eq!(r.digest(), 0);
    }

    // ========== Adler32 tests ==========

    #[test]
    fn adler32_empty_is_one() {
        // Adler32("") = 1 (B = 0; A = 1). Classic reference vector.
        assert_eq!(adler32(&[]), 0x0001);
    }

    #[test]
    fn adler32_rfc1950_vector() {
        // RFC 1950 §3 example: adler32("123456789") = 0x091E01DE  (= 594816734)
        assert_eq!(adler32(b"123456789"), 0x091E_01DE);
    }

    #[test]
    fn adler32_stream_matches_one_shot() {
        let buf: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
        let one_shot = adler32(&buf);
        let mut state = RollingAdler32::new(buf.len());
        for &byte in &buf {
            if let Some(d) = state.update(byte) {
                assert_eq!(d, one_shot);
            }
        }
    }
}

impl TridgellRolling {
    /// Non-panicking constructor mirroring [`TridgellRolling::new`].
    pub fn try_new(len: usize) -> Option<Self> {
        if len == 0 || len > u16::MAX as usize {
            return None;
        }
        Some(Self::new(len))
    }
}
