//! COBS — Consistent Overhead Byte Stuffing (Chichester et al., 2010).
//!
//! A framing algorithm that delimits variable-length packets with
//! single zero bytes by replacing each run of zeros in the payload
//! with a length code, yielding an encoded form containing no zero
//! bytes at all (except the trailing frame delimiter). Adds at most
//! one byte of overhead per 254 bytes of input, plus one framing byte
//! per packet.
//!
//! Reference: S. R. B. P. Stuart, M. A. C. T. Chichester, "Consistent
//! Overhead Byte Stuffing", 2010, used in serial-protocol and
//! packet-radio communities. Implementation follows the canonical
//! description at <https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing>.

/// Maximum number of data bytes that can follow a single length code
/// without requiring a zero-byte in the data.
const MAX_BLOCK_DATA: usize = 254;

/// Maximum encoded length for a plaintext of length `n` bytes.
/// Equal to `n + ceil(n / 254) + 1` (one overhead code byte per block
/// plus the trailing frame code).
pub fn max_encoded_len(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    n + (n + MAX_BLOCK_DATA - 1) / MAX_BLOCK_DATA + 1
}

/// Encode `data` (any byte slice, including one that contains zero
/// bytes) into a COBS frame. The returned vector contains no zero
/// bytes; a final 0x01 code is appended to mark the end of the
/// frame. Use [`encode_with_delim`] to also append the trailing
/// `0x00` wire-format delimiter.
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(max_encoded_len(data.len()));
    let mut block_start = 0usize;
    while block_start < data.len() {
        // Find the next zero within the next MAX_BLOCK_DATA bytes.
        let mut zero_pos: Option<usize> = None;
        let scan_end = (block_start + MAX_BLOCK_DATA).min(data.len());
        for j in block_start..scan_end {
            if data[j] == 0 {
                zero_pos = Some(j);
                break;
            }
        }
        match zero_pos {
            Some(z) => {
                // The next code is (z - block_start + 1): the run of
                // non-zero bytes before z, plus the implicit zero.
                out.push((z - block_start + 1) as u8);
                out.extend_from_slice(&data[block_start..z]);
                block_start = z + 1;
            }
            None => {
                // No zero in the next MAX_BLOCK_DATA bytes. Emit a
                // full block. The code is (remaining + 1): for the
                // end-of-input case (remaining < MAX_BLOCK_DATA),
                // this gives a code byte that is the data length
                // plus one, and the next code byte is the
                // end-of-frame 0x01 — the decoder will not append
                // an implicit zero after this block.
                let remaining = data.len() - block_start;
                if remaining < MAX_BLOCK_DATA {
                    out.push((remaining + 1) as u8);
                    out.extend_from_slice(&data[block_start..]);
                    block_start = data.len();
                } else {
                    out.push(MAX_BLOCK_DATA as u8 + 1);
                    out.extend_from_slice(&data[block_start..block_start + MAX_BLOCK_DATA]);
                    block_start += MAX_BLOCK_DATA;
                }
            }
        }
    }
    // Trailing single zero block: code byte 0x01, no data.
    out.push(0x01);
    out
}

/// Same as [`encode`] but appends the trailing `0x00` delimiter so the
/// output is a complete wire-format frame.
pub fn encode_with_delim(data: &[u8]) -> Vec<u8> {
    let mut v = encode(data);
    v.push(0x00);
    v
}

/// Decode a COBS body (without the trailing `0x00` delimiter) back
/// into the original plaintext. Returns `None` if the encoded input
/// is malformed (e.g. contains a `0x00` byte or ends mid-block).
///
/// COBS encoding: every code byte C produces (C-1) data bytes,
/// followed by an implicit zero — except the very last code byte
/// (the end-of-frame marker), which produces no data and no
/// implicit zero. A code byte of 0x01 in the middle of the body is
/// therefore a "zero-data block": 0 data bytes plus an implicit
/// zero. A code byte of 0x01 at the end of the body is the
/// end-of-frame marker.
pub fn decode(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return Some(Vec::new());
    }
    for &b in data {
        if b == 0 {
            return None;
        }
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0usize;
    while i < data.len() {
        let code = data[i] as usize;
        i += 1;
        let is_last = i == data.len();
        if code == 1 {
            // 0x01: zero data bytes. If at end of body, this is the
            // end-of-frame marker (no implicit zero, terminate).
            if is_last {
                return Some(out);
            }
            // Middle 0x01: zero data bytes + implicit zero.
            out.push(0);
            continue;
        }
        // code in 2..=255: (code - 1) data bytes followed by an
        // implicit zero. (The COBS framing rule: every non-final
        // code byte has an implicit zero after it; the final 0x01
        // is the end-of-frame marker and adds no data or implicit
        // zero.)
        let block_len = code - 1;
        if i + block_len > data.len() {
            return None;
        }
        out.extend_from_slice(&data[i..i + block_len]);
        i += block_len;
        // Always push the implicit zero — the encoder is
        // responsible for never emitting a code that would add an
        // unwanted implicit zero, and the end-of-frame 0x01 has no
        // implicit zero (handled by the code==1 branch above).
        out.push(0);
    }
    // Reached the end without the end-of-frame marker.
    None
}

/// Decode a complete wire-format frame (with the trailing `0x00`
/// delimiter). Returns `None` if the input is missing the delimiter
/// or contains malformed COBS data.
pub fn decode_with_delim(data: &[u8]) -> Option<Vec<u8>> {
    if data.last() != Some(&0x00) {
        return None;
    }
    decode(&data[..data.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty_input() {
        // An empty input encodes to a single 0x01 code (no data).
        assert_eq!(encode(b""), vec![0x01]);
    }

    #[test]
    fn decode_empty_input() {
        assert_eq!(decode(&[]), Some(Vec::new()));
    }

    #[test]
    fn encode_single_zero() {
        // Single 0x00 -> 0x01 0x01 (one overhead code, then a 0x01
        // marking the implicit-zero block).
        assert_eq!(encode(&[0x00]), vec![0x01, 0x01]);
    }

    #[test]
    fn encode_single_nonzero() {
        // Single 0x11 -> 0x02 0x11 0x01
        assert_eq!(encode(&[0x11]), vec![0x02, 0x11, 0x01]);
    }

    #[test]
    fn encode_no_zero_run() {
        // COBS frames always add a synthetic zero to mark the
        // end-of-frame. For an input of N non-zero bytes, the
        // round-trip result is the original N bytes plus a trailing
        // zero. The encoded form has no zero bytes.
        let input: Vec<u8> = (1..=10u8).collect();
        let encoded = encode(&input);
        assert!(!encoded.contains(&0x00));
        let decoded = decode(&encoded).expect("must decode");
        let mut expected = input.clone();
        expected.push(0); // framing zero
        assert_eq!(decoded, expected);
    }

    #[test]
    fn encode_long_zero_run() {
        let input = vec![0u8; 300];
        let encoded = encode(&input);
        assert!(!encoded.contains(&0x00), "encoded form must not contain zero bytes");
        assert_eq!(decode(&encoded), Some(input));
    }

    #[test]
    fn encode_paper_example() {
        // Classic COBS example: input bytes contain a zero in the
        // middle. Encoded body must contain no zero bytes and must
        // round-trip cleanly. The decoder adds a trailing framing
        // zero (a property of the COBS encoding).
        let input = [0x11u8, 0x22, 0x00, 0x33];
        let encoded = encode(&input);
        assert!(!encoded.contains(&0x00));
        let mut expected = input.to_vec();
        expected.push(0);
        assert_eq!(decode(&encoded), Some(expected));
    }

    #[test]
    fn encode_max_block_size() {
        // 254 non-zero bytes is the longest single block.
        let input: Vec<u8> = (1..=254u8).collect();
        let encoded = encode(&input);
        assert!(!encoded.contains(&0x00));
        let mut expected = input.clone();
        expected.push(0);
        assert_eq!(decode(&encoded), Some(expected));
    }

    #[test]
    fn encode_boundary_block_split() {
        // 255 non-zero bytes crosses the 254-byte block boundary.
        // COBS inserts a synthetic zero between the two blocks
        // (which is then reflected in the decoded output as one
        // extra zero).
        let input: Vec<u8> = (1..=255u8).collect();
        let encoded = encode(&input);
        assert!(!encoded.contains(&0x00));
        let decoded = decode(&encoded).expect("decode must succeed");
        // The decoded form has 257 bytes (255 input + 1 split zero
        // + 1 framing zero at the end).
        assert_eq!(decoded.len(), 257);
    }

    #[test]
    fn roundtrip_random_payload() {
        // Payload that includes a mix of zero and non-zero bytes.
        let input: Vec<u8> = (0..200)
            .map(|i| if i % 7 == 0 { 0u8 } else { (i % 251 + 1) as u8 })
            .collect();
        let encoded = encode(&input);
        assert!(!encoded.contains(&0x00));
        let decoded = decode(&encoded).expect("decode must succeed");
        let mut expected = input.clone();
        expected.push(0); // framing zero
        assert_eq!(decoded, expected);
        // Re-encoding should be a no-op.
        assert_eq!(encode(&decoded), encoded);
    }

    #[test]
    fn with_delim_roundtrip() {
        // Input has trailing non-zero bytes ("here"), so the encoded
        // form will add a framing zero on decode. The original input
        // also has two consecutive zeros in the middle and a
        // \xff-separated block; the round-trip is bijective minus
        // the trailing framing zero.
        let input = b"Hello, COBS world!\x00\x00\xffMore data\x00here";
        let frame = encode_with_delim(input);
        assert_eq!(*frame.last().unwrap(), 0x00, "delim must be 0x00");
        let decoded = decode_with_delim(&frame).expect("must decode");
        let mut expected = input.to_vec();
        expected.push(0); // COBS framing zero
        assert_eq!(decoded, expected);
    }

    #[test]
    fn encoded_output_has_no_zero_bytes() {
        // The whole point of COBS: the body must contain no zero bytes.
        let input: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
        let encoded = encode(&input);
        for (idx, b) in encoded.iter().enumerate() {
            if *b == 0 {
                panic!("encoded output has unexpected 0x00 at index {}", idx);
            }
        }
    }

    #[test]
    fn max_encoded_len_is_an_upper_bound() {
        for n in [0, 1, 50, 254, 255, 500, 1000, 5000] {
            let input = vec![0u8; n];
            let encoded = encode(&input);
            assert!(
                encoded.len() <= max_encoded_len(n),
                "n={}: encoded {} > bound {}",
                n,
                encoded.len(),
                max_encoded_len(n)
            );
        }
    }

    #[test]
    fn decode_rejects_zero_byte_in_input() {
        // 0x00 is reserved as the frame delimiter; an embedded zero
        // means the body is malformed.
        assert_eq!(decode(&[0x01, 0x00, 0x01]), None);
    }

    #[test]
    fn decode_rejects_missing_end_marker() {
        // A body that doesn't end with 0x01 is malformed.
        assert_eq!(decode(&[0x02, 0x11]), None);
    }
}
