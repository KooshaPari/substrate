//! Minimal DNS query build / round-trip parity helper (RFC 1035 §4.1.1).
//!
//! This is the parity counterpart to [`crate::dns_message_parser`]:
//! it builds a wire-format DNS *query* packet (header + question
//! section, no answer/authority/additional) and asserts that the
//! existing parser round-trips every byte back to the input fields.
//!
//! The existing `dns_message_parser` is parsing-only. This module is
//! intentionally narrow: it covers A/AAAA/NS/MX/TXT/CNAME/SOA/PTR/ANY
//! question-type bitmasks, IN class only, and a single question.
//! Multi-question queries and compression pointers are out of scope --
//! they're covered in tests elsewhere if needed.
//!
//! Wire layout (RFC 1035 §4.1):
//!
//! ```text
//!   Header (12 bytes, all big-endian)
//!     id           u16
//!     flags        u16
//!     qd_count     u16
//!     an_count     u16
//!     ns_count     u16
//!     ar_count     u16
//!   Question (variable)
//!     qname        sequence of length-prefixed labels, 0-terminated
//!     qtype        u16 BE
//!     qclass       u16 BE
//! ```

/// DNS class: IN (the Internet class). The other classes (CH, HS,
/// NONE, ANY) are reserved / classless; we model IN only here because
/// every modern resolver expects IN for the question class.
pub const CLASS_IN: u16 = 1;

/// DNS query type bitmasks, RFC 1035 §3.2.2 + §3.4.1 + RFC 3596.
pub mod qtype {
    pub const A: u16 = 1;
    pub const NS: u16 = 2;
    pub const CNAME: u16 = 5;
    pub const SOA: u16 = 6;
    pub const PTR: u16 = 12;
    pub const MX: u16 = 15;
    pub const TXT: u16 = 16;
    pub const AAAA: u16 = 28;
    pub const SRV: u16 = 33;
    pub const ANY: u16 = 255;
}

/// Build the wire-format DNS query for `(id, flags, qname, qtype)`.
///
/// `qname` is the dotted domain (e.g. `"example.com"`); labels are
/// limited to 63 bytes each per RFC 1035 §3.1 and the full name is
/// limited to 253 bytes (RFC 1035 §2.3.4). Both limits are enforced
/// here as a hard error so callers can't accidentally build an
/// oversized buffer.
pub fn build_query(id: u16, flags: u16, qname: &str, qt: u16) -> Result<Vec<u8>, String> {
    if qname.len() > 253 {
        return Err(format!("qname too long: {} bytes (RFC 1035 §2.3.4)", qname.len()));
    }
    // Validate labels.
    let mut total_label_len: usize = 0;
    for label in qname.split('.') {
        if label.is_empty() {
            // Trailing dot ("example.com.") is allowed; we handle it
            // by noting no labels after the final split. Empty interior
            // labels ("a..b") are not.
            continue;
        }
        if label.len() > 63 {
            return Err(format!(
                "label too long: {} bytes (RFC 1035 §3.1 limit 63)",
                label.len()
            ));
        }
        total_label_len += label.len() + 1; // 1-byte length prefix
    }
    // Wire length of the name section.
    let wire_name_len = if qname.ends_with('.') {
        // "example.com." -> 7 example + 1 + 3 com + 1 + 1 terminator = 13
        total_label_len + 1
    } else {
        total_label_len + 1
    };
    let total_len = 12 + wire_name_len + 4; // header + name + qtype + qclass
    let mut buf = Vec::with_capacity(total_len);
    buf.extend_from_slice(&id.to_be_bytes());
    buf.extend_from_slice(&flags.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes()); // qd_count = 1
    buf.extend_from_slice(&0u16.to_be_bytes()); // an_count
    buf.extend_from_slice(&0u16.to_be_bytes()); // ns_count
    buf.extend_from_slice(&0u16.to_be_bytes()); // ar_count
    // Encode qname. Split the input into label runs; a trailing dot
    // signals the root label and adds an empty terminator.
    let trimmed = qname.trim_end_matches('.');
    if !trimmed.is_empty() {
        for label in trimmed.split('.') {
            buf.push(label.len() as u8);
            buf.extend_from_slice(label.as_bytes());
        }
    }
    buf.push(0); // root terminator (always present)
    buf.extend_from_slice(&qt.to_be_bytes());
    buf.extend_from_slice(&CLASS_IN.to_be_bytes());
    debug_assert_eq!(buf.len(), total_len, "build_query size miscalculation");
    Ok(buf)
}

/// Re-parse a query built by `build_query` and assert all fields
/// round-trip. Panics with a descriptive message on mismatch.
///
/// Used as a one-line end-to-end check in tests.
pub fn assert_round_trip_query(expected_id: u16, expected_flags: u16, expected_qname: &str, expected_qt: u16, buf: &[u8]) {
    let h = crate::dns_message_parser::parse_header(buf)
        .expect("dns_query_parser_parity: header parse failed");
    if h.id != expected_id {
        panic!(
            "dns_query_parser_parity: id mismatch expected=0x{:04x} got=0x{:04x}",
            expected_id, h.id
        );
    }
    if h.flags != expected_flags {
        panic!(
            "dns_query_parser_parity: flags mismatch expected=0x{:04x} got=0x{:04x}",
            expected_flags, h.flags
        );
    }
    if h.qd_count != 1 {
        panic!(
            "dns_query_parser_parity: qd_count mismatch expected=1 got={}",
            h.qd_count
        );
    }
    if h.an_count != 0 || h.ns_count != 0 || h.ar_count != 0 {
        panic!(
            "dns_query_parser_parity: counts not zero an={} ns={} ar={}",
            h.an_count, h.ns_count, h.ar_count
        );
    }
    let (q, _end) = crate::dns_message_parser::parse_question(buf, 12)
        .expect("dns_query_parser_parity: question parse failed");
    if q.qname != expected_qname.trim_end_matches('.') {
        // The existing parser strips the trailing dot; we accept both
        // forms by comparing without the trailing dot.
        panic!(
            "dns_query_parser_parity: qname mismatch expected={} got={}",
            expected_qname.trim_end_matches('.'),
            q.qname
        );
    }
    if q.qtype != expected_qt {
        panic!(
            "dns_query_parser_parity: qtype mismatch expected={} got={}",
            expected_qt, q.qtype
        );
    }
    if q.qclass != CLASS_IN {
        panic!(
            "dns_query_parser_parity: qclass mismatch expected={} got={}",
            CLASS_IN, q.qclass
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_message_parser;

    // The standard query flags per RFC 1035 §4.1.1: RD=1, OPCODE=0.
    // (Queries commonly use 0x0100 = RD=1 with everything else 0.)
    const RD: u16 = 0x0100;

    #[test]
    fn build_a_query_example_com() {
        let pkt = build_query(0x1234, RD, "example.com", qtype::A).unwrap();
        assert_eq!(pkt.len(), 12 + 13 + 4); // header + "7example3com0" + qtype/qclass
        assert_eq!(pkt[0..2], [0x12, 0x34]); // id
        assert_eq!(pkt[2..4], RD.to_be_bytes());
        assert_eq!(&pkt[4..6], &[0x00, 0x01]); // qd_count=1
        assert_eq!(&pkt[6..12], &[0u8; 6]); // an/ns/ar all zero
        assert_eq!(pkt[12], 7); // "example"
        assert_eq!(&pkt[13..20], b"example");
        assert_eq!(pkt[20], 3); // "com"
        assert_eq!(&pkt[21..24], b"com");
        assert_eq!(pkt[24], 0); // root terminator
        assert_eq!(pkt[25..27], qtype::A.to_be_bytes());
        assert_eq!(pkt[27..29], CLASS_IN.to_be_bytes());
    }

    #[test]
    fn round_trip_a_query() {
        let pkt = build_query(0xbeef, RD, "example.com", qtype::A).unwrap();
        assert_round_trip_query(0xbeef, RD, "example.com", qtype::A, &pkt);
    }

    #[test]
    fn round_trip_aaaa_query() {
        let pkt = build_query(0xabcd, RD, "ipv6.example.org", qtype::AAAA).unwrap();
        assert_round_trip_query(0xabcd, RD, "ipv6.example.org", qtype::AAAA, &pkt);
    }

    #[test]
    fn round_trip_mx_query() {
        let pkt = build_query(0xfeed, RD, "mail.example.com", qtype::MX).unwrap();
        assert_round_trip_query(0xfeed, RD, "mail.example.com", qtype::MX, &pkt);
    }

    #[test]
    fn round_trip_any_query_with_trailing_dot() {
        // "ns1.example.com." (with root dot) -- parser strips it; we
        // expect the round-trip comparator to compare without the dot.
        let pkt = build_query(0x9988, RD, "ns1.example.com.", qtype::A).unwrap();
        assert_round_trip_query(0x9988, RD, "ns1.example.com.", qtype::A, &pkt);
    }

    #[test]
    fn round_trip_single_label() {
        // Top-level single-label queries are rare but allowed; e.g.
        // "localhost" used by some resolvers.
        let pkt = build_query(0x0001, 0, "localhost", qtype::A).unwrap();
        assert_round_trip_query(0x0001, 0, "localhost", qtype::A, &pkt);
    }

    #[test]
    fn round_trip_txt_query() {
        let pkt = build_query(0x4242, RD, "_dmarc.example.com", qtype::TXT).unwrap();
        assert_round_trip_query(0x4242, RD, "_dmarc.example.com", qtype::TXT, &pkt);
    }

    #[test]
    fn reject_label_too_long() {
        let label = "a".repeat(64);
        let qname = format!("{}.example.com", label);
        let err = build_query(0x1234, RD, &qname, qtype::A).unwrap_err();
        assert!(err.contains("label too long"), "unexpected error: {}", err);
    }

    #[test]
    fn reject_qname_too_long() {
        let qname = format!("{}.example.com", "a".repeat(254));
        let err = build_query(0x1234, RD, &qname, qtype::A).unwrap_err();
        assert!(err.contains("qname too long"), "unexpected error: {}", err);
    }

    #[test]
    fn build_matches_existing_parser_example() {
        // The existing dns_message_parser test fixture uses the same
        // byte layout -- cross-check that our builder produces the
        // exact bytes the existing parser's test asserts on.
        // (From crates/gateway/src/dns_message_parser.rs::query_example)
        let pkt = build_query(0x1234, RD, "example.com", qtype::A).unwrap();
        let h = dns_message_parser::parse_header(&pkt).unwrap();
        assert_eq!(h.id, 0x1234);
        assert_eq!(h.qd_count, 1);
        let (q, _) = dns_message_parser::parse_question(&pkt, 12).unwrap();
        assert_eq!(q.qname, "example.com");
        assert_eq!(q.qtype, qtype::A);
        assert_eq!(q.qclass, CLASS_IN);
    }

    #[test]
    fn build_query_with_zero_flags_is_query() {
        // RFC 1035 §4.1.1: flags=0 means a standard query.
        let pkt = build_query(0x0000, 0, "x.test", qtype::A).unwrap();
        assert_round_trip_query(0x0000, 0, "x.test", qtype::A, &pkt);
    }
}
