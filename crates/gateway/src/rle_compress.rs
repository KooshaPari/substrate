//! Run-length encoding (RLE) — lossless compression for repetitive data.
//!
//! A simple, deterministic scheme: each run of `n >= 1` consecutive equal
//! bytes `b` becomes `(n, b)`. We use a header byte for `n`:
//! `n >= 128` is emitted as a literal escape (see below). Otherwise the
//! pair `(n, b)` represents the run directly. The encoding is fully
//! reversible via `decode`.
//!
//! Format:
//!
//! ```text
//! stream := run*
//! run    := [0x01..=0x7F] byte           // short run, length 1..=127
//!        |  [0x80] [0x00..=0xFF] byte    // long run, length = byte2 + 128
//!        |  [0x81] count_lo count_hi     // literal escape: next (count+1) bytes are literal
//! ```
//!
//! The literal escape (`0x81`) carries `count+1` so `count = 0` means
//! "1 literal byte". Literal bytes follow directly and may be any byte
//! value, including those that would otherwise be ambiguous.
//!
//! Maximum single-run length: 256 + 127 = 383 bytes. Inputs whose run
//! length exceeds 383 are split across multiple run headers. This is a
//! portable wire format (no 16-/32-bit widths) so it round-trips on any
//! transport.

/// Maximum run length expressible in a single short or long run header.
pub const MAX_RUN: usize = 383;

/// Encode `data` to a byte stream using RLE. Always succeeds; the
/// output length is at most `2 * data.len() + 1` (literal escape worst
/// case: every byte preceded by the 3-byte escape header).
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    if data.is_empty() {
        return out;
    }
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        // Find run length (capped at MAX_RUN).
        let mut run = 1usize;
        while run < MAX_RUN && i + run < data.len() && data[i + run] == b {
            run += 1;
        }
        emit_run(&mut out, b, run);
        i += run;
    }
    out
}

fn emit_run(out: &mut Vec<u8>, b: u8, run: usize) {
    if run <= 127 {
        out.push(run as u8);
        out.push(b);
    } else {
        out.push(0x80);
        out.push((run - 128) as u8);
        out.push(b);
    }
}

/// Emit a literal escape for `chunk`. `chunk.len() - 1` is encoded in the
/// two count bytes (little-endian). Chunk length must be in `1..=65536`.
fn emit_literal(out: &mut Vec<u8>, chunk: &[u8]) {
    debug_assert!(!chunk.is_empty());
    let n = chunk.len() - 1;
    out.push(0x81);
    out.push((n & 0xFF) as u8);
    out.push(((n >> 8) & 0xFF) as u8);
    out.extend_from_slice(chunk);
}

/// Decode an RLE-encoded stream back to the original bytes. Returns
/// `Err(String)` if the stream is malformed (truncated, runs past end,
/// literal length exceeds remaining bytes).
pub fn decode(encoded: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < encoded.len() {
        let tag = encoded[i];
        i += 1;
        match tag {
            0x00 => return Err("rle: reserved tag 0x00".into()),
            0x01..=0x7F => {
                if i >= encoded.len() {
                    return Err("rle: truncated short run".into());
                }
                let n = tag as usize;
                let b = encoded[i];
                i += 1;
                for _ in 0..n {
                    out.push(b);
                }
            }
            0x80 => {
                if i + 2 > encoded.len() {
                    return Err("rle: truncated long run".into());
                }
                let n = (encoded[i] as usize) + 128;
                let b = encoded[i + 1];
                i += 2;
                for _ in 0..n {
                    out.push(b);
                }
            }
            0x81 => {
                if i + 2 > encoded.len() {
                    return Err("rle: truncated literal header".into());
                }
                let n = (encoded[i] as usize) | ((encoded[i + 1] as usize) << 8);
                i += 2;
                let total = n + 1;
                if i + total > encoded.len() {
                    return Err("rle: literal overruns stream".into());
                }
                out.extend_from_slice(&encoded[i..i + total]);
                i += total;
            }
            _ => return Err(format!("rle: unknown tag 0x{:02X}", tag)),
        }
    }
    Ok(out)
}

/// Compression ratio as `compressed / original`. Returns `None` if the
/// input is empty (ratio undefined).
pub fn ratio(orig_len: usize, comp_len: usize) -> Option<f64> {
    if orig_len == 0 {
        None
    } else {
        Some(comp_len as f64 / orig_len as f64)
    }
}

/// Encode with literal fallback for non-repeating regions. Uses RLE for
/// any run of `>= MIN_RUN` identical bytes and emits a literal escape
/// for everything else. This avoids the pathological 2x expansion of
/// pure RLE on random data.
pub const MIN_RUN: usize = 3;

pub fn encode_adaptive(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        // Collect maximal run.
        let mut run = 1usize;
        while run < MAX_RUN && i + run < data.len() && data[i + run] == data[i] {
            run += 1;
        }
        if run >= MIN_RUN {
            emit_run(&mut out, data[i], run);
            i += run;
        } else {
            // Emit a literal chunk up to the next run or 4096 bytes.
            let mut chunk_end = i;
            let cap = (i + 4096).min(data.len());
            while chunk_end < cap {
                let mut peek_run = 1usize;
                while peek_run < MAX_RUN && chunk_end + peek_run < cap && data[chunk_end + peek_run] == data[chunk_end] {
                    peek_run += 1;
                }
                if peek_run >= MIN_RUN {
                    break;
                }
                chunk_end += 1;
            }
            // Always emit at least one literal byte.
            if chunk_end == i {
                chunk_end = (i + 1).min(data.len());
            }
            // Split into chunks of <= 65536 bytes (literal header limit).
            let mut start = i;
            while start < chunk_end {
                let take = (chunk_end - start).min(65536);
                emit_literal(&mut out, &data[start..start + take]);
                start += take;
            }
            i = chunk_end;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_round_trip() {
        let enc = encode(&[]);
        assert!(enc.is_empty());
        let dec = decode(&enc).unwrap();
        assert!(dec.is_empty());
    }

    #[test]
    fn single_byte() {
        let enc = encode(&[0x42]);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, vec![0x42]);
    }

    #[test]
    fn short_run_round_trip() {
        let data = vec![b'A'; 5];
        let enc = encode(&data);
        // 1 short-run header (2 bytes).
        assert_eq!(enc, vec![5, b'A']);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn long_run_uses_long_header() {
        let data = vec![b'B'; 200];
        let enc = encode(&data);
        // 200 = 128 + 72; long header is 3 bytes: [0x80, 72, 'B']
        assert_eq!(enc, vec![0x80, 72, b'B']);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn run_at_max_length() {
        let data = vec![b'C'; MAX_RUN];
        let enc = encode(&data);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn run_exceeding_max_is_split() {
        let data = vec![b'D'; MAX_RUN + 50];
        let enc = encode(&data);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
        // Header sequence should be: long(383) + short(50).
        assert_eq!(enc[0], 0x80);
        assert_eq!(enc[3], 50);
    }

    #[test]
    fn multiple_runs_interleaved() {
        let mut data = vec![b'a'; 10];
        data.extend(vec![b'b'; 3]);
        data.extend(vec![b'c'; 200]);
        data.extend(vec![b'd'; 7]);
        let enc = encode(&data);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn alternating_bytes_still_round_trip() {
        let data: Vec<u8> = (0..50).map(|i| if i % 2 == 0 { 0xAA } else { 0x55 }).collect();
        let enc = encode(&data);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn literal_round_trip() {
        // Manually craft a literal escape for bytes that would be
        // ambiguous as run headers.
        let mut enc = Vec::new();
        enc.push(0x81);
        enc.push(3); // n=3 -> 4 literal bytes follow
        enc.push(0);
        enc.extend_from_slice(&[0x00, 0x80, 0x81, 0x42]);
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, vec![0x00, 0x80, 0x81, 0x42]);
    }

    #[test]
    fn adaptive_no_expansion_on_random() {
        // Random-ish data should not balloon.
        let mut data = Vec::new();
        let mut x: u8 = 1;
        for _ in 0..1000 {
            // xorshift-ish; deterministic but high-entropy.
            x ^= x << 3;
            x ^= x >> 2;
            x ^= x << 1;
            data.push(x);
        }
        let enc = encode_adaptive(&data);
        assert!(enc.len() < data.len() * 2, "adaptive encoder expanded by 2x or more");
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn adaptive_compresses_repetitive() {
        let mut data = Vec::new();
        data.extend(vec![b'X'; 500]);
        data.extend(vec![b'Y'; 100]);
        data.extend(vec![b'Z'; 1000]);
        let enc = encode_adaptive(&data);
        assert!(enc.len() < data.len() / 2, "adaptive did not compress");
        let dec = decode(&enc).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn decode_truncated_short_run_errors() {
        let bad = vec![5]; // short-run header without payload byte
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn decode_truncated_long_run_errors() {
        let bad = vec![0x80, 0x10]; // missing payload byte
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn decode_literal_overrun_errors() {
        let bad = vec![0x81, 10, 0]; // claims 11 literal bytes, none provided
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn ratio_helper() {
        assert_eq!(ratio(0, 0), None);
        assert!((ratio(100, 50).unwrap() - 0.5).abs() < 1e-9);
        assert!((ratio(100, 200).unwrap() - 2.0).abs() < 1e-9);
    }
}