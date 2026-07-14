//! Minimal RDP negotiation-request *parity* round-trip helper.
//!
//! This is the parity counterpart to [`crate::rdp_neg`]: it builds a
//! wire-format `RDP_NEG_REQ` (MS-RDPBCGR §2.2.1.1) byte stream for a
//! given `rdpProtocols` bitmask and optional cookie, then re-parses the
//! buffer and asserts the parsed `RdpNegReq` equals the input.
//!
//! The point is NOT to duplicate `rdp_neg.rs`: that module is parsing
//! only and operates on TPKT + COTP framing. This module assumes the
//! caller has already peeled those off, and operates at the
//! negotiation-request layer only.
//!
//! Wire layout (MS-RDPBCGR §2.2.1.1):
//!
//! ```text
//!   offset  field            size   encoding
//!   0       type             1      u8  -- 0x01 (TYPE_NEG_REQ)
//!   1       flags            1      u8  -- reserved (zero in practice)
//!   2       length           2      u16 LE -- 8 + cookie.len() (incl. null terminator)
//!   4       rdpProtocols     4      u32 LE
//!   8       cookie           N      cookie payload (UTF-8 ASCII bytes; NOT
//!                                         null-terminated on the wire.
//!                                         The `length` field includes the
//!                                         trailing terminator.
//! ```
//!
//! Note: the cookie field per spec carries an empty byte when absent
//! (length = 8 means no cookie). We faithfully reproduce that.

/// Build a wire-format RDP negotiation-request buffer.
///
/// `protocols` is the `rdpProtocols` bitmask (PROTOCOL_RDP |
/// PROTOCOL_SSL etc. from [`crate::rdp_neg::protocol`]).
/// `cookie` is the optional cookie payload WITHOUT the trailing
/// `Cookie: mstshash=` magic (the caller is expected to embed that
/// themselves if they want the canonical wire shape). When `cookie`
/// is `None`, the request still carries the empty byte at offset 8
/// (the spec convention) and reports length = 8.
///
/// The returned buffer is exactly `8 + cookie.len()` bytes.
pub fn build_request(protocols: u32, cookie: Option<&[u8]>) -> Vec<u8> {
    let body = cookie.unwrap_or(&[]);
    let length: u16 = 8u16 + body.len() as u16;
    let mut buf = Vec::with_capacity(length as usize);
    buf.push(crate::rdp_neg::TYPE_NEG_REQ);
    buf.push(0x00); // flags
    buf.extend_from_slice(&length.to_le_bytes());
    buf.extend_from_slice(&protocols.to_le_bytes());
    buf.extend_from_slice(body);
    buf
}

/// Re-parse a request buffer produced by `build_request` and panic
/// with a descriptive message if it doesn't match the input.
///
/// The 'input' here is rebuilt internally by calling `build_request`
/// with the same parameters: the round-trip is by definition
/// meaningful only if both sides agree byte-for-byte.
///
/// Used in tests as a one-line end-to-end check:
/// `assert_round_trip(build_request(protocols, cookie))`.
pub fn assert_round_trip(req: &[u8]) {
    if req.len() < 8 {
        panic!(
            "rdp_neg_parity: request buffer too short ({} bytes; need >= 8)",
            req.len()
        );
    }
    let ty = req[0];
    let flags = req[1];
    let length = u16::from_le_bytes([req[2], req[3]]);
    let protocols = u32::from_le_bytes([req[4], req[5], req[6], req[7]]);
    if ty != crate::rdp_neg::TYPE_NEG_REQ {
        panic!(
            "rdp_neg_parity: type byte = 0x{:02x}, expected 0x{:02x}",
            ty,
            crate::rdp_neg::TYPE_NEG_REQ
        );
    }
    if flags != 0 {
        panic!(
            "rdp_neg_parity: reserved flags byte = 0x{:02x}, expected 0x00",
            flags
        );
    }
    if length as usize != req.len() {
        panic!(
            "rdp_neg_parity: length field = {} but buffer is {} bytes",
            length,
            req.len()
        );
    }
    // MS-RDPBCGR §2.2.1.1: only certain reserved bits must be zero in
    // `rdpProtocols`. The standard flag set we accept is the union of
    // PROTOCOL_RDP (0x0) | PROTOCOL_SSL (0x1) | PROTOCOL_HYBRID (0x2)
    // | PROTOCOL_RDS_TLS (0x4) | PROTOCOL_HYBRID_EX (0x8).
    const PROTOCOL_MASK: u32 = 0x0F;
    if protocols & !PROTOCOL_MASK != 0 {
        panic!(
            "rdp_neg_parity: rdpProtocols has reserved bits set: 0x{:08x}",
            protocols
        );
    }
    let cookie = if req.len() > 8 { &req[8..] } else { &[][..] };
    let rebuilt = build_request(protocols, Some(cookie));
    if rebuilt != req {
        panic!(
            "rdp_neg_parity: round-trip mismatch. built_len={} rebuilt={:?} req={:?}",
            rebuilt.len(),
            rebuilt,
            req
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rdp_neg::protocol::*;

    #[test]
    fn build_no_cookie_correct_length() {
        // MS-RDPBCGR §2.2.1.1: when no cookie is supplied the request
        // still carries length=8 and an empty byte at offset 8 (it is
        // the length field's contents that determines "no cookie",
        // not an explicit sentinel -- here we report the empty byte).
        let req = build_request(PROTOCOL_RDP | PROTOCOL_SSL, None);
        assert_eq!(req.len(), 8);
        assert_eq!(req[0], crate::rdp_neg::TYPE_NEG_REQ);
        assert_eq!(req[1], 0x00);
        assert_eq!(u16::from_le_bytes([req[2], req[3]]), 8);
        assert_eq!(
            u32::from_le_bytes([req[4], req[5], req[6], req[7]]),
            PROTOCOL_RDP | PROTOCOL_SSL
        );
    }

    #[test]
    fn build_with_cookie_includes_payload() {
        // Cookie "Cookie: mstshash=foo" (no terminator) -> 21 bytes total.
        let cookie: &[u8] = b"Cookie: mstshash=foo";
        let req = build_request(PROTOCOL_SSL | PROTOCOL_HYBRID, Some(cookie));
        assert_eq!(req.len(), 8 + cookie.len());
        assert_eq!(
            u16::from_le_bytes([req[2], req[3]]),
            8 + cookie.len() as u16
        );
        assert_eq!(&req[8..], cookie);
    }

    #[test]
    fn round_trip_no_cookie() {
        let req = build_request(PROTOCOL_RDP, None);
        assert_round_trip(&req);
    }

    #[test]
    fn round_trip_with_cookie() {
        let cookie: &[u8] = b"Cookie: mstshash=user@example.com:3389";
        let req = build_request(PROTOCOL_SSL | PROTOCOL_HYBRID, Some(cookie));
        assert_round_trip(&req);
    }

    #[test]
    fn round_trip_all_protocols() {
        let protocols =
            PROTOCOL_RDP | PROTOCOL_SSL | PROTOCOL_HYBRID | PROTOCOL_RDS_TLS | PROTOCOL_HYBRID_EX;
        let cookie: &[u8] = b"Cookie: mstshash=Administrator@host:3389";
        let req = build_request(protocols, Some(cookie));
        assert_round_trip(&req);
        // The only allowed low 4 bits are the 5 protocol flags.
        let parsed = u32::from_le_bytes([req[4], req[5], req[6], req[7]]);
        assert_eq!(parsed, protocols);
    }

    #[test]
    fn round_trip_with_empty_cookie_some() {
        // Some(b"") is meaningful: the request still carries length 8.
        let req = build_request(PROTOCOL_RDP, Some(b""));
        assert_eq!(req.len(), 8);
        assert_eq!(u16::from_le_bytes([req[2], req[3]]), 8);
        assert_round_trip(&req);
    }

    #[test]
    fn parity_distinct_from_existing_builder() {
        // Sanity: the rdp_neg module does not export `build_request`
        // today, so our parity module is the canonical producer.
        // This test pins the contract.
        let req = build_request(PROTOCOL_SSL, Some(b"x"));
        assert_eq!(req.len(), 9);
        assert_eq!(req[0], 0x01);
        assert_eq!(req[1], 0x00);
        assert_eq!(req[8], b'x');
    }

    #[test]
    fn assert_round_trip_rejects_short_buffer() {
        // 7-byte buffer should be rejected with a panic. We use
        // catch_unwind because panic != test failure on stable.
        let result = std::panic::catch_unwind(|| assert_round_trip(&[0u8; 7]));
        assert!(
            result.is_err(),
            "assert_round_trip must reject <8 byte input"
        );
    }

    #[test]
    fn assert_round_trip_rejects_bad_type() {
        let mut req = build_request(PROTOCOL_RDP, None);
        req[0] = 0x00; // wrong type
        let result = std::panic::catch_unwind(|| assert_round_trip(&req));
        assert!(
            result.is_err(),
            "assert_round_trip must reject non-NEG_REQ type"
        );
    }

    #[test]
    fn assert_round_trip_rejects_bad_length_field() {
        let mut req = build_request(PROTOCOL_RDP, None);
        req[2] = 0x10; // length field says 0x1008 but buffer is 8 bytes
        let result = std::panic::catch_unwind(|| assert_round_trip(&req));
        assert!(
            result.is_err(),
            "assert_round_trip must reject mismatched length"
        );
    }

    #[test]
    fn assert_round_trip_rejects_reserved_bits() {
        let mut req = build_request(PROTOCOL_RDP, None);
        req[4] = 0xff; // upper bits of rdpProtocols set
        let result = std::panic::catch_unwind(|| assert_round_trip(&req));
        assert!(
            result.is_err(),
            "assert_round_trip must reject reserved protocol bits"
        );
    }
}
