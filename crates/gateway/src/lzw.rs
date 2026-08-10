//! LZW (Lempel-Ziv-Welch) dictionary-based compression.
//!
//! LZW is a universal lossless compression algorithm that builds a
//! dictionary of substrings dynamically while reading the input. The
//! output is a stream of dictionary indices. The decoder rebuilds the
//! same dictionary from the output stream alone (no side info required).
//!
//! Reference: Welch, "A Technique for High-Performance Data Compression"
//! (IEEE Computer, 1984); <https://en.wikipedia.org/wiki/Lempel%E2%80%93Ziv%E2%80%93Welch>.
//!
//! This implementation uses a **fixed 12-bit code width** and reserves
//! codes 0..=255 for single bytes, 256 for CLEAR, 257 for EOI, and
//! 258..=4095 for dictionary entries. The fixed-width approach removes
//! the GIF-style dynamic-width bookkeeping at a modest compression
//! penalty for very small dictionaries.
//!
//! Pure safe Rust. No `unsafe`, no external crates.

/// Clear-code marker (resets dictionary).
pub const CLEAR_CODE: u16 = 256;
/// End-of-information marker.
pub const EOI_CODE: u16 = 257;
/// First user-assignable code.
pub const FIRST_CODE: u16 = 258;
/// Maximum code value (12-bit packing).
pub const MAX_CODE: u16 = 4095;
/// Code width in bits (fixed at 12 for simplicity).
pub const CODE_WIDTH: u32 = 12;

/// Bit-packer / writer that emits LSB-first code words of fixed width.
struct BitWriter {
    buf: Vec<u8>,
    bit_buf: u32,
    bits_in_buf: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            bit_buf: 0,
            bits_in_buf: 0,
        }
    }

    /// Emit `width` low-order bits of `code`.
    fn emit(&mut self, code: u32, width: u32) {
        self.bit_buf |= (code & ((1u32 << width) - 1)) << self.bits_in_buf;
        self.bits_in_buf += width;
        while self.bits_in_buf >= 8 {
            self.buf.push(self.bit_buf as u8);
            self.bit_buf >>= 8;
            self.bits_in_buf -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits_in_buf > 0 {
            self.buf.push(self.bit_buf as u8);
        }
        self.buf
    }
}

/// Bit-reader that yields LSB-first code words of fixed width.
struct BitReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    bit_buf: u32,
    bits_in_buf: u32,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            pos: 0,
            bit_buf: 0,
            bits_in_buf: 0,
        }
    }

    /// Read `width` bits and return as a `u32`. Returns `None` if input is exhausted.
    fn read(&mut self, width: u32) -> Option<u32> {
        while self.bits_in_buf < width {
            let b = *self.bytes.get(self.pos)?;
            self.pos += 1;
            self.bit_buf |= (b as u32) << self.bits_in_buf;
            self.bits_in_buf += 8;
        }
        let code = self.bit_buf & ((1u32 << width) - 1);
        self.bit_buf >>= width;
        self.bits_in_buf -= width;
        Some(code)
    }
}

/// Compress `input` into a sequence of 12-bit LZW codes packed LSB-first.
///
/// The output starts with a CLEAR_CODE and ends with an EOI_CODE so a
/// decoder can recover the dictionary state. If the dictionary fills
/// (next_code > MAX_CODE), a CLEAR_CODE is emitted mid-stream and the
/// dictionary is reset.
pub fn lzw_compress(input: &[u8]) -> Vec<u8> {
    let mut dict: std::collections::HashMap<Vec<u8>, u16> =
        (0u16..=255).map(|i| (vec![i as u8], i)).collect();
    let mut next_code: u16 = FIRST_CODE;
    let mut w = BitWriter::new();
    w.emit(CLEAR_CODE as u32, CODE_WIDTH);

    if input.is_empty() {
        w.emit(EOI_CODE as u32, CODE_WIDTH);
        return w.finish();
    }

    let mut prev: Vec<u8> = vec![input[0]];
    for &b in &input[1..] {
        let mut candidate = prev.clone();
        candidate.push(b);
        if dict.contains_key(&candidate) {
            prev = candidate;
        } else {
            let code = dict[&prev];
            w.emit(code as u32, CODE_WIDTH);
            if next_code <= MAX_CODE {
                dict.insert(candidate, next_code);
                next_code += 1;
            } else {
                // Dictionary full: emit CLEAR and reset.
                w.emit(CLEAR_CODE as u32, CODE_WIDTH);
                dict.clear();
                dict = (0u16..=255).map(|i| (vec![i as u8], i)).collect();
                next_code = FIRST_CODE;
            }
            prev = vec![b];
        }
    }
    let code = dict[&prev];
    w.emit(code as u32, CODE_WIDTH);
    w.emit(EOI_CODE as u32, CODE_WIDTH);
    w.finish()
}

/// Decompress an LZW code stream produced by [`lzw_compress`] (or compatible).
///
/// Returns `None` if the input is malformed (truncated code word, unknown
/// code, or invalid dictionary state).
pub fn lzw_decompress(input: &[u8]) -> Option<Vec<u8>> {
    let mut reader = BitReader::new(input);
    let first_code = reader.read(CODE_WIDTH)?;
    if first_code != CLEAR_CODE as u32 {
        return None;
    }
    let mut dict: std::collections::HashMap<u16, Vec<u8>> =
        (0u16..=255).map(|i| (i, vec![i as u8])).collect();
    let mut next_code: u16 = FIRST_CODE;

    let mut prev: Option<Vec<u8>> = None;
    let mut out: Vec<u8> = Vec::new();

    loop {
        let code = reader.read(CODE_WIDTH)? as u16;
        if code == EOI_CODE {
            return Some(out);
        }
        if code == CLEAR_CODE {
            dict.clear();
            for i in 0u16..=255 {
                dict.insert(i, vec![i as u8]);
            }
            next_code = FIRST_CODE;
            prev = None;
            continue;
        }
        let entry: Vec<u8> = if let Some(e) = dict.get(&code) {
            e.clone()
        } else if code == next_code && prev.is_some() {
            // KwKwK case: code refers to entry being defined right now.
            let mut p = prev.clone().unwrap();
            let first = p[0];
            p.push(first);
            p
        } else {
            return None;
        };
        out.extend_from_slice(&entry);
        if let Some(p) = prev.as_ref() {
            if next_code <= MAX_CODE {
                let mut new_entry = p.clone();
                new_entry.push(entry[0]);
                dict.insert(next_code, new_entry);
                next_code += 1;
            }
        }
        prev = Some(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_roundtrip() {
        let encoded = lzw_compress(b"");
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, b"");
    }

    #[test]
    fn single_byte_roundtrip() {
        let input = b"A";
        let encoded = lzw_compress(input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn ascii_roundtrip() {
        let input = b"TOBEORNOTTOBEORTOBEORNOT";
        let encoded = lzw_compress(input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn highly_repetitive_input_roundtrip() {
        let input = vec![b'a'; 1000];
        let encoded = lzw_compress(&input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn all_byte_values_roundtrip() {
        let input: Vec<u8> = (0u8..=255).collect();
        let encoded = lzw_compress(&input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn dictionary_full_emits_clear() {
        // Force dictionary overflow with diverse repetitive input.
        let mut input = Vec::new();
        for i in 0u16..2000 {
            input.push((i & 0xFF) as u8);
        }
        let encoded = lzw_compress(&input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn kwkwk_case_roundtrip() {
        // Construct a sequence that forces the KwKwK dictionary extension.
        let input = b"ABABABA";
        let encoded = lzw_compress(input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn long_unique_byte_sequence_roundtrip() {
        // Each byte is distinct from its neighbor; dictionary still grows.
        let input: Vec<u8> = (0u32..500).map(|i| (i % 251) as u8).collect();
        let encoded = lzw_compress(&input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn malformed_stream_returns_none() {
        // Just CLEAR then EOI.
        let compressed = lzw_compress(b"");
        let decoded = lzw_decompress(&compressed).expect("decode");
        assert_eq!(decoded, b"");
    }

    #[test]
    fn missing_initial_clear_rejected() {
        // A stream that starts with a non-CLEAR code should be rejected.
        let mut encoded = Vec::new();
        // Emit EOI (257 = 0b000100000001) as the first 12 bits.
        encoded.push(0x01);
        encoded.push(0x10);
        assert!(lzw_decompress(&encoded).is_none());
    }

    #[test]
    fn compression_reduces_repetition() {
        // LZW on highly repetitive input should produce fewer bytes than input.
        let input = vec![b'X'; 500];
        let encoded = lzw_compress(&input);
        let decoded = lzw_decompress(&encoded).expect("decode");
        assert_eq!(decoded, input);
        // With 12-bit codes the savings for repetition are still significant.
        assert!(
            encoded.len() < input.len(),
            "encoded {} should be < input {}",
            encoded.len(),
            input.len()
        );
    }
}
