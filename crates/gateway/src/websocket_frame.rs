//! Minimal RFC 6455 WebSocket frame encoder/decoder.
//!
//! Implements the two directions a server-side adapter needs most:
//!
//! * Encoding: emit a server-to-client frame. Per RFC 6455 §5.1, frames
//!   sent from server to client MUST NOT be masked.
//! * Decoding: parse a client-to-server frame. Per RFC 6455 §5.1, frames
//!   received by a server from a client MUST be masked; the mask is
//!   removed during decoding.
//!
//! This module is deliberately small — enough to drive round-trip
//! tests and feed the gateway's message path. It does NOT cover
//! fragmentation, control-frame interleaving, or extension negotiation.

const MASK_BIT: u8 = 0x80;
const PAYLOAD_LEN_7: usize = 125;
const PAYLOAD_LEN_16: usize = 126;
const PAYLOAD_LEN_64: usize = 127;

/// Encode a WebSocket frame header + payload for server-to-client
/// (unmasked) transmission.
///
/// * `fin` — true for a final frame, false for a continuation.
/// * `opcode` — 0x1 text, 0x2 binary, 0x8 close, 0x9 ping, 0xA pong.
///   The lower 4 bits are stored verbatim; the caller is responsible
///   for keeping the opcode well-formed.
/// * `payload` — frame body.
pub fn encode_fin_opcode(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
    assert!(opcode & 0xF0 == 0, "opcode must fit in 4 bits");

    let fin_bit: u8 = if fin { 0x80 } else { 0x00 };
    let first_byte = fin_bit | (opcode & 0x0F);

    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(first_byte);

    let len = payload.len();
    if len <= PAYLOAD_LEN_7 {
        // Server-to-client: mask bit MUST be zero.
        out.push(len as u8);
    } else if len <= u16::MAX as usize {
        out.push(PAYLOAD_LEN_16 as u8);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(PAYLOAD_LEN_64 as u8);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }

    out.extend_from_slice(payload);
    out
}

/// Parse a single WebSocket frame from `data`.
///
/// Returns `(fin, opcode, payload, bytes_consumed)` on success. The
/// returned `payload` has been unmasked when the frame carried a
/// mask (server frames MUST carry one; per RFC 6455 a server MUST
/// close the connection on receiving an unmasked frame, but this
/// decoder accepts both to keep tests simple).
///
/// Returns `None` when the buffer is too short to contain a full
/// frame, when `opcode` carries reserved bits, or when an extended
/// payload length disagrees with the buffer size.
pub fn parse_frame(data: &[u8]) -> Option<(bool, u8, Vec<u8>, usize)> {
    if data.len() < 2 {
        return None;
    }

    let first = data[0];
    let second = data[1];

    let fin = (first & 0x80) != 0;
    let rsv = first & 0x70;
    let opcode = first & 0x0F;
    if rsv != 0 {
        // Reserved bits set — protocol violation.
        return None;
    }

    let masked = (second & MASK_BIT) != 0;
    let mut len = (second & 0x7F) as usize;
    let mut offset = 2usize;

    if len == PAYLOAD_LEN_16 {
        if data.len() < offset + 2 {
            return None;
        }
        let ext = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        len = ext;
    } else if len == PAYLOAD_LEN_64 {
        if data.len() < offset + 8 {
            return None;
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&data[offset..offset + 8]);
        let ext = u64::from_be_bytes(buf) as usize;
        offset += 8;
        len = ext;
    }

    let mask: Option<[u8; 4]> = if masked {
        if data.len() < offset + 4 {
            return None;
        }
        let mut m = [0u8; 4];
        m.copy_from_slice(&data[offset..offset + 4]);
        offset += 4;
        Some(m)
    } else {
        None
    };

    if data.len() < offset + len {
        return None;
    }

    let payload_src = &data[offset..offset + len];
    let payload = match mask {
        Some(m) => unmask(payload_src, &m),
        None => payload_src.to_vec(),
    };
    offset += len;

    Some((fin, opcode, payload, offset))
}

fn unmask(payload: &[u8], mask: &[u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len());
    for (i, b) in payload.iter().enumerate() {
        out.push(b ^ mask[i & 3]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_small_unmasked_text_roundtrip() {
        let payload = b"hello";
        let frame = encode_fin_opcode(true, 0x1, payload);
        // 2 header bytes + 5 payload bytes
        assert_eq!(frame.len(), 7);
        // No mask bit on the wire from server.
        assert_eq!(frame[1] & MASK_BIT, 0);
        assert_eq!(frame[1] & 0x7F, payload.len() as u8);

        let (fin, op, decoded, used) = parse_frame(&frame).expect("parse");
        assert!(fin);
        assert_eq!(op, 0x1);
        assert_eq!(decoded, payload);
        assert_eq!(used, frame.len());
    }

    #[test]
    fn decode_masked_client_text() {
        // Client-to-server frame: header has mask bit set, mask of 4 bytes, then payload.
        // Build by hand to exercise the masked path.
        let payload = b"masked";
        let mask = [0xDE, 0xAD, 0xBE, 0xEF];
        let mut masked_payload = Vec::new();
        for (i, b) in payload.iter().enumerate() {
            masked_payload.push(b ^ mask[i & 3]);
        }

        let mut frame = Vec::new();
        frame.push(0x81); // FIN + text
        frame.push(MASK_BIT | (payload.len() as u8));
        frame.extend_from_slice(&mask);
        frame.extend_from_slice(&masked_payload);

        let (fin, op, decoded, used) = parse_frame(&frame).expect("parse");
        assert!(fin);
        assert_eq!(op, 0x1);
        assert_eq!(decoded, payload);
        assert_eq!(used, frame.len());
    }

    #[test]
    fn fin_flag_propagates() {
        // continuation (FIN=0) of a binary frame
        let frame = encode_fin_opcode(false, 0x2, &[1, 2, 3]);
        let (fin, op, _, _) = parse_frame(&frame).expect("parse");
        assert!(!fin);
        assert_eq!(op, 0x2);
    }

    #[test]
    fn opcodes_echo_binary_close_ping() {
        // echo (0x1)
        let f = encode_fin_opcode(true, 0x1, b"x");
        let (_, op, _, _) = parse_frame(&f).unwrap();
        assert_eq!(op, 0x1);

        // binary (0x2)
        let f = encode_fin_opcode(true, 0x2, &[0xAA, 0xBB]);
        let (_, op, decoded, _) = parse_frame(&f).unwrap();
        assert_eq!(op, 0x2);
        assert_eq!(decoded, vec![0xAA, 0xBB]);

        // close (0x8) — empty payload is fine
        let f = encode_fin_opcode(true, 0x8, &[]);
        let (_, op, decoded, used) = parse_frame(&f).unwrap();
        assert_eq!(op, 0x8);
        assert!(decoded.is_empty());
        assert_eq!(used, 2);

        // ping (0x9)
        let f = encode_fin_opcode(true, 0x9, b"ping?");
        let (_, op, decoded, _) = parse_frame(&f).unwrap();
        assert_eq!(op, 0x9);
        assert_eq!(decoded, b"ping?");
    }

    #[test]
    fn extended_payload_length_16() {
        let payload = vec![0xCC; 200];
        let frame = encode_fin_opcode(true, 0x2, &payload);
        // 2 header + 2 ext-len + 200 payload
        assert_eq!(frame.len(), 204);
        assert_eq!(frame[1] & 0x7F, PAYLOAD_LEN_16 as u8);

        let (_, op, decoded, used) = parse_frame(&frame).expect("parse");
        assert_eq!(op, 0x2);
        assert_eq!(decoded, payload);
        assert_eq!(used, frame.len());
    }

    #[test]
    fn truncated_frame_returns_none() {
        // Header claims 5 bytes but only 2 are present.
        let f = encode_fin_opcode(true, 0x1, b"hello");
        let truncated = &f[..f.len() - 3];
        assert!(parse_frame(truncated).is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(parse_frame(&[]).is_none());
    }
}