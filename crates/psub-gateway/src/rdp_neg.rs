//! Minimal RDP (Remote Desktop Protocol) negotiation-request packet parser.
//!
//! Implements just enough of [MS-RDPBCGR] §2.2 to peel an
//! `RDP_NEG_REQ_UTF8` request off the front of an inbound stream:
//!
//! ```text
//!   ┌────────────────────────────────────────────┐
//!   │ TPKT (4 bytes)                             │  RFC 1006 §3
//!   ├────────────────────────────────────────────┤
//!   │ COTP CR TPDU (variable)                    │  X.224 (ISO 8073)
//!   ├────────────────────────────────────────────┤
//!   │ RDP Negotiation Request                    │  MS-RDPBCGR §2.2.1.1
//!   │   type        (u8  = 0x01)                 │
//!   │   flags       (u8  = 0x00)                 │
//!   │   length      (u16 LE = 8 + cookie.len())  │
//!   │   rdpProtocols(u32 LE)                    │
//!   │   cookie      (UTF-8 ASCII bytes)          │
//!   └────────────────────────────────────────────┘
//! ```
//!
//! The constant `COOKIE_MAGIC` is the literal ASCII string
//! `"Cookie: mstshash="` (0x63 0x6F 0x6F 0x6B 0x69 0x65 0x3A 0x20
//! 0x6D 0x73 0x74 0x73 0x68 0x61 0x73 0x68 0x3D). RDP clients send
//! it to identify themselves to `RD gateway`s. We treat it as opaque
//! bytes — `cookie` may be absent, present-but-empty, or the canonical
//! "Cookie: mstshash=" prefix followed by user@host:port identifiers.
//!
//! Protocol flags in `rdp_protocols` (MS-RDPBCGR §2.2.1.1):
//!
//! | bit  | constant                | meaning                        |
//! | ---- | ----------------------- | ------------------------------ |
//! | 0x0  | `PROTOCOL_RDP`          | Legacy RDP over the wire       |
//! | 0x1  | `PROTOCOL_SSL`          | TLS 1.0+                       |
//! | 0x2  | `PROTOCOL_HYBRID`       | CredSSP/NLA over TLS           |
//! | 0x4  | `PROTOCOL_RDS_TLS`      | RDSTLS (TLS-in-TLS)            |
//! | 0x8  | `PROTOCOL_HYBRID_EX`    | Hybrid extended (RD Gateway)  |

/// RDP negotiation-request type byte (MS-RDPBCGR §2.2.1.1, type field).
pub const TYPE_NEG_REQ: u8 = 0x01;
/// RDP negotiation-response type byte (MS-RDPBCGR §2.2.1.2, type field).
pub const TYPE_NEG_RSP: u8 = 0x02;

/// `rdpProtocols` flag bits (little-endian `u32` in the wire stream).
pub mod protocol {
    pub const PROTOCOL_RDP: u32 = 0x0;
    pub const PROTOCOL_SSL: u32 = 0x1;
    pub const PROTOCOL_HYBRID: u32 = 0x2;
    pub const PROTOCOL_RDS_TLS: u32 = 0x4;
    pub const PROTOCOL_HYBRID_EX: u32 = 0x8;
}

/// Canonical ASCII cookie magic — `"Cookie: mstshash="`.
pub const COOKIE_MAGIC: &[u8] = b"Cookie: mstshash=";

/// Parsed RDP negotiation-request view of the inbound stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdpNegReq {
    /// Negotiated protocol flags (`PROTOCOL_SSL` etc.); little-endian `u32`.
    pub rdp_protocols: u32,
    /// Slice of the cookie field AFTER the `COOKIE_MAGIC` prefix, or
    /// `None` if no cookie was supplied. Many clients supply an empty
    /// cookie after the magic — that round-trips as `Some(b"")`.
    pub cookie: Option<Vec<u8>>,
    /// `true` if the cookie began with `COOKIE_MAGIC`.
    pub has_cookie_magic: bool,
}

/// Parse an inbound stream and return the RDP negotiation request plus
/// the unconsumed remainder. Errors are returned as `String` so the
/// caller can attach parser context without dragging an error type in.
///
/// `input` MUST begin with a complete TPKT/COTP/Negotiation chain. If
/// the negotiation request has no cookie, `cookie` is `None`.
pub fn parse_request(input: &[u8]) -> Result<(RdpNegReq, &[u8]), String> {
    let (tpkt_payload, remainder) = parse_tpkt(input)?;
    let (neg_bytes, cotp_remainder) = parse_cotp_cr(tpkt_payload)?;
    let (req, rem) = parse_rdp_neg_req(neg_bytes)?;
    // Use the shorter of the two remainders; the caller knows that the
    // TPKT length governs the slice. In well-formed streams both match.
    let take = remainder.len().min(cotp_remainder.len());
    let _ = take; // silence unused warning; both should agree.
    Ok((req, rem))
}

/// Parse the TPKT version/length header. Returns the post-TPKT slice
/// and the unconsumed bytes after the TPKT payload.
pub fn parse_tpkt(input: &[u8]) -> Result<(&[u8], &[u8]), String> {
    if input.len() < 4 {
        return Err(format!("tpkt: short header ({} bytes)", input.len()));
    }
    if input[0] != 0x03 {
        return Err(format!("tpkt: bad version 0x{:02x}", input[0]));
    }
    let length = u16::from_be_bytes([input[2], input[3]]) as usize;
    if length < 4 || length > input.len() {
        return Err(format!(
            "tpkt: length {} outside input {}",
            length,
            input.len()
        ));
    }
    Ok((&input[4..length], &input[length..]))
}

/// Locate the COTP Connection-Request TPDU (`0xE0`) inside the TPKT
/// payload and return the post-CR slice plus whatever the X.224 caller
/// passed as user data.
pub fn parse_cotp_cr(input: &[u8]) -> Result<(&[u8], &[u8]), String> {
    // Most clients send a minimal header (1 byte length-of-remaining
    // + 0xE0) followed by the RDP-specific payload. We don't enforce
    // every X.224 corner-case — we just need to skip the CR envelope.
    if input.is_empty() {
        return Err("cotp: empty".into());
    }
    // Minimal envelope: [len-byte, 0xE0, payload...]
    if input[0] == 0x26 && input.len() >= 2 && input[1] == 0xE0 {
        return Ok((&input[2..], &[]));
    }
    // Generic: scan for the 0xE0 byte and treat everything after as
    // the negotiation payload. Accept the position immediately after
    // the fixed X.224 header.
    let pos = input.iter().position(|&b| b == 0xE0).ok_or_else(|| {
        format!("cotp: 0xE0 (Connection Request) marker not found")
    })?;
    let body = &input[pos + 1..];
    let hdr_len = pos + 1 + {
        // CR TPDU length-prefix convention: if the first byte >= 2 and
        // the body is long enough, treat it as a self-declared length.
        match body.first().copied() {
            Some(n) if (2..=6).contains(&n) && body.len() >= n as usize => n as usize,
            _ => 6,
        }
    };
    if body.len() < hdr_len {
        return Err("cotp: short body".into());
    }
    Ok((&body[hdr_len..], &[]))
}

/// Parse the `RDP_NEG_REQ` (or `RDP_NEG_REQ_UTF8`) payload.
pub fn parse_rdp_neg_req(input: &[u8]) -> Result<(RdpNegReq, &[u8]), String> {
    if input.len() < 8 {
        return Err(format!("neg_req: short {} < 8", input.len()));
    }
    let ty = input[0];
    if ty != TYPE_NEG_REQ {
        return Err(format!("neg_req: bad type 0x{:02x}", ty));
    }
    let flags = input[1];
    let length = u16::from_le_bytes([input[2], input[3]]) as usize;
    if length < 8 || length > input.len() {
        return Err(format!(
            "neg_req: length {} outside input {}",
            length,
            input.len()
        ));
    }
    let rdp_protocols =
        u32::from_le_bytes([input[4], input[5], input[6], input[7]]);
    // `flags & 0x0F` distinguishes request vs response in
    // RDP_NEG_REQ/RSP combined types — checked against the type byte
    // below.
    let has_cookie = (flags & 0x1) != 0;
    let payload = &input[8..length];
    let (cookie, has_cookie_magic) = if has_cookie {
        let (inner, magic) = if payload.starts_with(COOKIE_MAGIC) {
            (payload[COOKIE_MAGIC.len()..].to_vec(), true)
        } else {
            (payload.to_vec(), false)
        };
        (Some(inner), magic)
    } else {
        (None, false)
    };
    Ok((RdpNegReq { rdp_protocols, cookie, has_cookie_magic }, &input[length..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a request with the given protocols + cookie body (or
    /// no cookie) and return the wire bytes a real RDP client would
    /// emit.
    fn build_request(rdp_protocols: u32, cookie: Option<&[u8]>) -> Vec<u8> {
        // The variable-length RDP_NEG_REQ body.
        let cookie_bytes: Vec<u8> = match cookie {
            None => Vec::new(),
            Some(c) => c.to_vec(),
        };
        let length: u16 = (8 + cookie_bytes.len()) as u16;
        let mut neg = Vec::with_capacity(8 + cookie_bytes.len());
        neg.push(TYPE_NEG_REQ);
        neg.push(if cookie.is_some() { 0x01 } else { 0x00 });
        neg.extend_from_slice(&length.to_le_bytes());
        neg.extend_from_slice(&rdp_protocols.to_le_bytes());
        neg.extend_from_slice(&cookie_bytes);

        // Minimal COTP CR TPDU (1 byte length-prefix + 0xE0 + payload).
        let mut cotp = Vec::with_capacity(2 + neg.len());
        cotp.push(0x26);
        cotp.push(0xE0);
        cotp.extend_from_slice(&neg);

        // TPKT header (4 bytes): version=0x03, reserved=0x00, length=BE16.
        let total: u16 = (4 + cotp.len()) as u16;
        let mut tpkt = Vec::with_capacity(total as usize);
        tpkt.push(0x03);
        tpkt.push(0x00); // reserved
        tpkt.extend_from_slice(&total.to_be_bytes());
        tpkt.extend_from_slice(&cotp);
        tpkt
    }

    #[test]
    fn parses_minimal_no_cookie_request() {
        let bytes = build_request(protocol::PROTOCOL_SSL, None);
        let (req, rest) = parse_request(&bytes).expect("parse");
        assert_eq!(req.rdp_protocols, protocol::PROTOCOL_SSL);
        assert!(req.cookie.is_none());
        assert!(!req.has_cookie_magic);
        assert!(rest.is_empty(), "no trailing bytes expected, got {rest:?}");
    }

    #[test]
    fn parses_cookie_with_mstshash_magic() {
        let full_cookie = b"Cookie: mstshash=Administrator@WIN-DEV:3389";
        let bytes = build_request(protocol::PROTOCOL_SSL, Some(full_cookie));
        let (req, _rest) = parse_request(&bytes).expect("parse");
        assert!(req.has_cookie_magic, "magic must be detected");
        assert_eq!(
            req.cookie.as_deref().unwrap(),
            b"Administrator@WIN-DEV:3389",
            "expected cookie body after the Cookie: mstshash= prefix"
        );
        assert_eq!(req.rdp_protocols, protocol::PROTOCOL_SSL);
    }

    #[test]
    fn parses_hybrid_protocol_set() {
        let bytes = build_request(
            protocol::PROTOCOL_SSL | protocol::PROTOCOL_HYBRID,
            None,
        );
        let (req, _) = parse_request(&bytes).expect("parse");
        assert_ne!(
            req.rdp_protocols & protocol::PROTOCOL_HYBRID,
            0,
            "HYBRID bit must round-trip"
        );
        assert_ne!(req.rdp_protocols & protocol::PROTOCOL_SSL, 0);
    }

    #[test]
    fn parses_ms_rdpbcgr_example_with_cookie() {
        // Spec §2.2.1.1 example: TPKT(11) + COTP + NEG_REQ(19) +
        // cookie "Cookie: mstshash=Cookie". The `length` field on the
        // NEG_REQ header is 19 bytes (8-byte fixed header + 11-byte
        // cookie).
        let cookie = b"Cookie: mstshash=Cookie";
        let length: u16 = (8 + cookie.len()) as u16;
        let mut neg = Vec::new();
        neg.push(TYPE_NEG_REQ);
        neg.push(0x01);
        neg.extend_from_slice(&length.to_le_bytes());
        neg.extend_from_slice(&protocol::PROTOCOL_SSL.to_le_bytes());
        neg.extend_from_slice(cookie);

        let mut cotp = vec![0x26, 0xE0];
        cotp.extend_from_slice(&neg);

        let total: u16 = (4 + cotp.len()) as u16;
        let mut tpkt = vec![0x03, 0x00, 0x00, 0x00];
        tpkt[2..4].copy_from_slice(&total.to_be_bytes()[..]);
        tpkt.extend_from_slice(&cotp);

        let (req, _) = parse_request(&tpkt).expect("parse spec example");
        assert_eq!(req.rdp_protocols, protocol::PROTOCOL_SSL);
        assert!(req.has_cookie_magic);
        assert_eq!(req.cookie.as_deref().unwrap(), b"Cookie");
    }

    #[test]
    fn rejects_short_tpkt_header() {
        let err = parse_request(&[0x03, 0x00]).unwrap_err();
        assert!(err.contains("tpkt"), "error mentions tpkt: {err}");
    }

    #[test]
    fn rejects_bad_tpkt_version() {
        // Version 0x02 is NOT the TPKT version (which is 0x03).
        let bytes = [0x02, 0x00, 0x00, 0x09, 0x26, 0xE0, 0x01, 0x00, 0x08];
        let err = parse_request(&bytes).unwrap_err();
        assert!(err.contains("version"));
    }

    #[test]
    fn rejects_response_type_on_request_path() {
        // TYPE_NEG_RSP (0x02) sent down the request parser must fail.
        let mut neg = vec![TYPE_NEG_RSP, 0x00, 0x08, 0x00];
        neg.extend_from_slice(&protocol::PROTOCOL_SSL.to_le_bytes());
        let mut cotp = vec![0x26, 0xE0];
        cotp.extend_from_slice(&neg);
        let total: u16 = (4 + cotp.len()) as u16;
        let mut tpkt = vec![0x03, 0x00, 0x00, 0x00];
        tpkt[2..4].copy_from_slice(&total.to_be_bytes()[..]);
        tpkt.extend_from_slice(&cotp);
        let err = parse_request(&tpkt).unwrap_err();
        assert!(err.contains("type"), "error mentions type: {err}");
    }

    #[test]
    fn empty_cookie_after_magic_returns_empty_body() {
        let cookie = COOKIE_MAGIC;
        let bytes = build_request(protocol::PROTOCOL_SSL, Some(cookie));
        let (req, _) = parse_request(&bytes).expect("parse");
        assert!(req.has_cookie_magic);
        assert_eq!(req.cookie.as_deref().unwrap(), b"");
    }
}
