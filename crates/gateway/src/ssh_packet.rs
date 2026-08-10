//! SSH binary packet protocol encoder/decoder (RFC 4253 section 6).
//!
//! Minimal hand-rolled implementation for testing purposes only.
//! No compression (`packet_length` always includes a 1-byte
//! `padding_length` field but no `packet_length` compression prefix).
//! No encryption or MAC — packets only carry an explicit MAC placeholder
//! slot (zero-filled) so callers can append real authentication bytes
//! later if desired.
//!
//! Wire layout produced by [`encode`]:
//!
//! ```text
//! uint32    packet_length    (length of `padding_length || payload || padding`, excluding MAC)
//! byte      padding_length
//! byte[]    payload
//! byte[]    random padding   (4 ..= 255 bytes)
//! byte[]    mac              (always 0 bytes from this encoder)
//! ```
//!
//! [`encode`]: crate::ssh_packet::encode
//! [`decode`]: crate::ssh_packet::decode

/// Minimum block size for the cipher (RFC 4253 §6).
const MIN_PADDING_BLOCK: usize = 8;
/// Minimum padding length per RFC 4253 §6.
const MIN_PADDING_LEN: usize = 4;
/// Maximum packet payload length (excluding padding/length field) we accept
/// from callers. 256 KiB is more than enough for tests and prevents the
/// encoder from emitting absurd `packet_length` values.
const MAX_PAYLOAD_LEN: usize = 256 * 1024;

/// Encode `message` into an SSH binary packet (no compression, no encryption,
/// no MAC).
///
/// `payload` is whatever bytes the caller wants to carry inside the packet.
/// The function chooses a random padding length that satisfies the RFC
/// (`>= 4`) and rounds the total `(1 + payload + padding)` length up to the
/// next multiple of the cipher block size (`8`).
pub fn encode(message: &[u8]) -> Vec<u8> {
    let payload_len = message.len();
    assert!(
        payload_len <= MAX_PAYLOAD_LEN,
        "ssh_packet::encode payload too large: {} > {}",
        payload_len,
        MAX_PAYLOAD_LEN
    );

    // Total "inside" length = 1 (padding_length field) + payload + padding.
    // Must be a multiple of MIN_PADDING_BLOCK and >= 1 + payload + 4.
    let min_inside = 1 + payload_len + MIN_PADDING_LEN;
    let blocks = (min_inside + MIN_PADDING_BLOCK - 1) / MIN_PADDING_BLOCK;
    let inside = blocks * MIN_PADDING_BLOCK;
    let padding_len = inside - 1 - payload_len;

    let mut out = Vec::with_capacity(4 + inside);
    let packet_length = inside as u32;
    out.extend_from_slice(&packet_length.to_be_bytes());
    out.push(padding_len as u8);
    out.extend_from_slice(message);
    // Deterministic, but pseudo-random padding. We do not need cryptographic
    // randomness here — the padding is purely structural and not used for
    // any security property in this minimal implementation.
    for i in 0..padding_len {
        // Mix index + payload bytes to avoid all-zero padding which could
        // be mistaken for missing trailing data in test fixtures.
        let seed = (i as u8).wrapping_add(payload_len as u8).wrapping_mul(31);
        out.push(seed);
    }
    out
}

/// Decode a packet produced by [`encode`].
///
/// Returns the payload bytes and the number of bytes consumed from
/// `packet` (including the 4-byte length prefix and MAC placeholder if
/// `with_mac` was used; this minimal decoder always treats the input as
/// MAC-less so the consumed length equals the full input).
///
/// [`encode`]: crate::ssh_packet::encode
pub fn decode(packet: &[u8]) -> Result<(Vec<u8>, usize), String> {
    if packet.len() < 5 {
        return Err(format!(
            "ssh_packet::decode packet too short: {} bytes (need >= 5)",
            packet.len()
        ));
    }

    let packet_length = u32::from_be_bytes([packet[0], packet[1], packet[2], packet[3]]) as usize;
    if packet_length < 1 {
        return Err(format!(
            "ssh_packet::decode invalid packet_length {}",
            packet_length
        ));
    }
    if packet_length > MAX_PAYLOAD_LEN + 256 {
        return Err(format!(
            "ssh_packet::decode packet_length {} exceeds maximum",
            packet_length
        ));
    }

    let expected_total = 4 + packet_length;
    if packet.len() < expected_total {
        return Err(format!(
            "ssh_packet::decode truncated packet: have {} bytes, need {}",
            packet.len(),
            expected_total
        ));
    }

    let padding_length = packet[4] as usize;
    let inside = packet_length;
    if padding_length >= inside {
        return Err(format!(
            "ssh_packet::decode invalid padding_length {} (inside {})",
            padding_length, inside
        ));
    }
    if padding_length < MIN_PADDING_LEN {
        return Err(format!(
            "ssh_packet::decode padding_length {} below minimum {}",
            padding_length, MIN_PADDING_LEN
        ));
    }

    let payload_start = 5;
    let payload_end = payload_start + (inside - 1 - padding_length);
    if payload_end > packet.len() {
        return Err("ssh_packet::decode payload range out of bounds".to_string());
    }

    let payload = packet[payload_start..payload_end].to_vec();
    Ok((payload, expected_total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_short_payload_has_minimum_padding() {
        let pkt = encode(b"hi");
        // packet_length = 1 (pad_len) + 2 (payload) + padding
        // minimum inside = 8 -> padding = 5
        assert_eq!(pkt.len(), 4 + 8, "unexpected total length");
        let packet_length = u32::from_be_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]) as usize;
        assert_eq!(packet_length, 8);
        assert_eq!(pkt[4], 5, "padding length should be 5");
        assert_eq!(&pkt[5..7], b"hi", "payload bytes");
    }

    #[test]
    fn encode_exact_block_size_payload() {
        // 7 bytes payload -> inside must be 16 (next multiple of 8).
        let payload = b"abcdefg";
        let pkt = encode(payload);
        let packet_length = u32::from_be_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]) as usize;
        assert_eq!(packet_length, 16, "inside should round up to 16");
        assert_eq!(pkt[4], 8, "padding length should be 8");
        assert_eq!(&pkt[5..12], payload, "payload bytes");
    }

    #[test]
    fn padding_varies_and_is_never_all_zero() {
        let pkt = encode(b"abcdefghij"); // 10 bytes
        let padding_length = pkt[4] as usize;
        // 1 + 10 + padding must be a multiple of 8 and >= 1 + 10 + 4 = 15.
        // Next multiple of 8 >= 15 is 16 -> padding = 5.
        assert_eq!(padding_length, 5);
        let padding_start = 5 + 10;
        let padding_end = padding_start + padding_length;
        let padding = &pkt[padding_start..padding_end];
        assert!(
            padding.iter().any(|b| *b != 0),
            "padding should not be all zero"
        );
    }

    #[test]
    fn round_trip_known_lengths() {
        for n in [0usize, 1, 7, 8, 9, 100, 1024, 4096] {
            let payload = vec![0xABu8; n];
            let pkt = encode(&payload);
            let (decoded, consumed) = decode(&pkt).expect("decode should succeed");
            assert_eq!(decoded, payload, "round trip mismatch for n={}", n);
            assert_eq!(
                consumed,
                pkt.len(),
                "consumed should equal pkt.len() for n={}",
                n
            );
        }
    }

    #[test]
    fn round_trip_arbitrary_bytes() {
        // Build a payload that exercises all byte values.
        let payload: Vec<u8> = (0u32..1024).map(|i| (i & 0xFF) as u8).collect();
        let pkt = encode(&payload);
        let (decoded, _) = decode(&pkt).expect("decode should succeed");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_rejects_truncated_packet() {
        let pkt = encode(b"hello world");
        let truncated = &pkt[..pkt.len() - 2];
        let err = decode(truncated).expect_err("should fail on truncated input");
        assert!(err.contains("truncated"), "unexpected error: {}", err);
    }

    #[test]
    fn decode_rejects_short_packet() {
        let err = decode(&[0, 0, 0]).expect_err("should fail on <5 bytes");
        assert!(err.contains("too short"), "unexpected error: {}", err);
    }

    #[test]
    fn decode_rejects_invalid_padding_length() {
        // Build a packet where padding_length >= inside. inside=8 -> padding_length=9 invalid.
        let mut bad = Vec::new();
        bad.extend_from_slice(&8u32.to_be_bytes());
        bad.push(9); // padding_length > inside-1
        bad.extend_from_slice(&[0u8; 7]); // padding bytes
        let err = decode(&bad).expect_err("should reject bad padding_length");
        assert!(err.contains("padding_length"), "unexpected error: {}", err);
    }

    #[test]
    fn decode_rejects_under_minimum_padding() {
        // inside = 8, padding_length = 3 (below min of 4)
        let mut bad = Vec::new();
        bad.extend_from_slice(&8u32.to_be_bytes());
        bad.push(3);
        bad.extend_from_slice(&[0u8; 7]);
        let err = decode(&bad).expect_err("should reject under-minimum padding");
        assert!(err.contains("below minimum"), "unexpected error: {}", err);
    }
}
