//! (7,4) Hamming code: single-bit error correction (SEC) and SECDED.
//!
//! The classical (7,4) Hamming code uses 4 message bits and computes 3
//! parity bits so the encoded 7-bit codeword lives in a 3-dimensional
//! parity subspace. Any single-bit error flips exactly enough parity
//! bits to identify which bit flipped. SECDED adds one extra overall
//! parity bit to also detect two-bit errors.
//!
//! References:
//! * Hamming, R. W. (1950) "Error detecting and error correcting codes"
//! * Lin & Costello, "Error Control Coding", 2nd ed., section 5.1
//!
//! Bit layout (low-to-high, 1-indexed positions in the comment column):
//!
//! | position | bit | meaning    |
//! | 1        | 0   | p1         |
//! | 2        | 1   | p2         |
//! | 3        | 2   | d1 (lsb)   |
//! | 4        | 3   | p4         |
//! | 5        | 4   | d2         |
//! | 6        | 5   | d3         |
//! | 7        | 6   | d4 (msb)   |
//! | 8        | 7   | (overall, for SECDED only) |

/// Encode 4 message bits into a 7-bit Hamming codeword.
///
/// `data` uses bits 0..=3 in the low nibble. Returns a `u8` whose bits
/// 0..=6 form the codeword (bit 0 = `p1`).
pub fn encode(data: u8) -> u8 {
    let d1 = (data >> 0) & 1;
    let d2 = (data >> 1) & 1;
    let d3 = (data >> 2) & 1;
    let d4 = (data >> 3) & 1;
    let p1 = d1 ^ d2 ^ d4;
    let p2 = d1 ^ d3 ^ d4;
    let p4 = d2 ^ d3 ^ d4;
    let mut cw: u8 = 0;
    cw |= p1 << 0;
    cw |= p2 << 1;
    cw |= d1 << 2;
    cw |= p4 << 3;
    cw |= d2 << 4;
    cw |= d3 << 5;
    cw |= d4 << 6;
    cw
}

/// Decode a 7-bit codeword (low 7 bits of input). Returns `(data, syndrome, error_bit)`.
///
/// `error_bit` is `None` for a clean codeword; otherwise it is the
/// 1-indexed position of the flipped bit (`1..=7`).
pub fn decode(codeword: u8) -> (u8, u8, Option<u8>) {
    let cw = codeword & 0x7F;
    let s1 = ((cw >> 0) & 1)
        ^ ((cw >> 2) & 1)
        ^ ((cw >> 4) & 1)
        ^ ((cw >> 6) & 1);
    let s2 = ((cw >> 1) & 1)
        ^ ((cw >> 2) & 1)
        ^ ((cw >> 5) & 1)
        ^ ((cw >> 6) & 1);
    let s4 = ((cw >> 3) & 1)
        ^ ((cw >> 4) & 1)
        ^ ((cw >> 5) & 1)
        ^ ((cw >> 6) & 1);
    let syndrome = s1 | (s2 << 1) | (s4 << 2);
    let cw = if syndrome != 0 {
        cw ^ (1u8 << (syndrome - 1))
    } else {
        cw
    };
    let d1 = (cw >> 2) & 0x01;
    let d2 = (cw >> 4) & 0x01;
    let d3 = (cw >> 5) & 0x01;
    let d4 = (cw >> 6) & 0x01;
    let data = d1 | (d2 << 1) | (d3 << 2) | (d4 << 3);
    let err = if syndrome == 0 { None } else { Some(syndrome) };
    (data, syndrome, err)
}

/// Convenience: SEC encode -> flip one bit -> SEC decode and recover.
pub fn correct_single_bit_error(data: u8, flip_position: u8) -> u8 {
    let cw = encode(data) ^ (1u8 << (flip_position - 1));
    let (out, _, _) = decode(cw);
    out
}

/// SECDED: like the 7-bit Hamming code but adds an overall parity bit
/// (position 8). Two-bit errors return syndrome 0 but overall parity
/// flips, signalling an *uncorrectable* error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecdedResult {
    Ok { data: u8 },
    Corrected { data: u8, error_bit: u8 },
    DoubleError,
}

pub fn encode_secded(data: u8) -> u8 {
    let cw = encode(data);
    // Overall even parity: bit 7 = (population count of cw) mod 2.
    let p_overall = (cw.count_ones() & 1) as u8;
    cw | (p_overall << 7)
}

pub fn decode_secded(byte: u8) -> SecdedResult {
    let byte = byte & 0xFF;
    let overall_parity = (byte.count_ones() & 1) as u8;
    let c = byte & 0x7F;
    let s1 = ((c >> 0) & 1)
        ^ ((c >> 2) & 1)
        ^ ((c >> 4) & 1)
        ^ ((c >> 6) & 1);
    let s2 = ((c >> 1) & 1)
        ^ ((c >> 2) & 1)
        ^ ((c >> 5) & 1)
        ^ ((c >> 6) & 1);
    let s4 = ((c >> 3) & 1)
        ^ ((c >> 4) & 1)
        ^ ((c >> 5) & 1)
        ^ ((c >> 6) & 1);
    let syndrome = s1 | (s2 << 1) | (s4 << 2);
    let decode_data = |cw: u8| -> u8 {
        let d1 = (cw >> 2) & 0x01;
        let d2 = (cw >> 4) & 0x01;
        let d3 = (cw >> 5) & 0x01;
        let d4 = (cw >> 6) & 0x01;
        d1 | (d2 << 1) | (d3 << 2) | (d4 << 3)
    };
    match (syndrome, overall_parity) {
        // Clean codeword.
        (0, 0) => SecdedResult::Ok { data: decode_data(c) },
        // Syndrome nonzero AND overall parity flipped -> single-bit error
        // in the Hamming portion. Correct it.
        (s, 1) if s != 0 => {
            let corrected = c ^ (1u8 << (s - 1));
            SecdedResult::Corrected { data: decode_data(corrected), error_bit: s }
        }
        // Syndrome zero but overall parity flipped -> bit 7 (overall
        // parity bit) flipped. The Hamming portion is intact.
        (0, _) => SecdedResult::Corrected { data: decode_data(c), error_bit: 8 },
        // Syndrome nonzero but overall parity unchanged -> two-bit
        // error pattern (errors cancel parity). Uncorrectable.
        (_, 0) => SecdedResult::DoubleError,
        // Remaining cases fall through as uncorrectable.
        (_, _) => SecdedResult::DoubleError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_all_zero_input() {
        // d=0000 -> all parity bits 0, all data bits 0.
        assert_eq!(encode(0x0), 0x00);
    }

    #[test]
    fn encode_known_textbook_vectors() {
        // Reference: d=0001 (d1=1, d2=d3=d4=0).
        // p1 = d1^d2^d4 = 1, p2 = d1^d3^d4 = 1, p4 = d2^d3^d4 = 0.
        // Codeword bits low..high: p1=1, p2=1, d1=1, p4=0, d2=0, d3=0, d4=0
        //                       = 0b0000_0111 = 0x07.
        assert_eq!(encode(0b0001), 0b0000_0111);
        // d=0011 (d1=1, d2=1, d3=0, d4=0).
        // p1 = 1^1^0 = 0, p2 = 1^0^0 = 1, p4 = 1^0^0 = 1.
        // Bits low..high: p1=0, p2=1, d1=1, p4=1, d2=1, d3=0, d4=0
        //              = 0b0001_1110 = 0x1E.
        assert_eq!(encode(0b0011), 0b0001_1110);
        // d=1010 (d1=0, d2=1, d3=0, d4=1).
        // p1 = 0^1^1 = 0, p2 = 0^0^1 = 1, p4 = 1^0^1 = 0.
        // Bits low..high: p1=0, p2=1, d1=0, p4=0, d2=1, d3=0, d4=1
        //              = 0b0101_0010 = 0x52.
        assert_eq!(encode(0b1010), 0b0101_0010);
        // d=1111 (d1=d2=d3=d4=1).
        // p1 = 1, p2 = 1, p4 = 1.
        // Bits low..high: 1,1,1,1,1,1,1 = 0b0111_1111 = 0x7F.
        assert_eq!(encode(0b1111), 0b0111_1111);
    }

    #[test]
    fn decode_clean_codewords_are_identity() {
        for d in 0u8..16 {
            let cw = encode(d);
            let (out, syndrome, err) = decode(cw);
            assert_eq!(out, d, "d={d:04b} cw={cw:07b}");
            assert_eq!(syndrome, 0, "d={d:04b} cw={cw:07b}");
            assert!(err.is_none(), "d={d:04b} cw={cw:07b}");
        }
    }

    #[test]
    fn decode_corrects_every_single_bit_flip() {
        for d in 0u8..16 {
            for bit in 1u8..=7 {
                let cw = encode(d) ^ (1 << (bit - 1));
                let (out, _, err) = decode(cw);
                assert_eq!(out, d, "d={d:04b}, flipped bit {bit}");
                assert_eq!(err, Some(bit), "d={d:04b}, bit {bit}");
            }
        }
    }

    #[test]
    fn correct_single_bit_error_helper() {
        // Convenience wrapper covers all positions.
        for d in 0u8..16 {
            for bit in 1u8..=7 {
                assert_eq!(correct_single_bit_error(d, bit), d);
            }
        }
    }

    #[test]
    fn seccded_clean_payload() {
        for d in 0u8..16 {
            let byte = encode_secded(d);
            assert_eq!(decode_secded(byte), SecdedResult::Ok { data: d });
        }
    }

    #[test]
    fn seccded_single_bit_corrects() {
        // d=0b1010 with each possible single-bit flip in the full
        // 8-bit SECDED codeword; decoder must report Corrected and
        // recover the message.
        let d = 0b1010u8;
        let byte = encode_secded(d);
        for bit in 0u8..8 {
            let corrupted = byte ^ (1 << bit);
            match decode_secded(corrupted) {
                SecdedResult::Corrected { data, error_bit } => {
                    assert_eq!(data, d, "bit={bit}");
                    assert_eq!(error_bit, bit + 1, "bit={bit}");
                }
                other => panic!(
                    "expected Corrected for d={d:04b} bit={bit}, got {other:?}"
                ),
            }
        }
    }

    #[test]
    fn seccded_double_error_detected() {
        // Construct a clean codeword then flip *two* bits in the
        // 7-bit codeword. The decoder must surface DoubleError.
        let d = 0b0101u8;
        let byte = encode_secded(d);
        // Flip bits 2 and 5 (within the 7-bit Hamming portion). These
        // both affect parity in nontrivial ways so syndrome != 0.
        let corrupted = byte ^ 0b0010_0100;
        match decode_secded(corrupted) {
            SecdedResult::DoubleError => {}
            other => panic!("expected DoubleError, got {other:?}"),
        }
        // Sanity: the underlying Hamming-only decoder sees a wrong
        // syndrome (nonzero).
        let (raw, syndrome, _) = decode(corrupted);
        assert_ne!(syndrome, 0);
        assert_ne!(raw, d);
    }
}
