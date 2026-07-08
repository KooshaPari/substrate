// Minimal STUN (Session Traversal Utilities for NAT, RFC 5389) message parser.
//
// Header (20 bytes):
//   bytes  0..2  Message Type (u16, big-endian)
//   bytes  2..4  Message Length (u16, big-endian) — length of payload after header
//   bytes  4..8  Magic Cookie (always 0x2112A442)
//   bytes  8..20 Transaction ID (12 bytes, opaque)
//
// STUN detection: the two most-significant bits of the first byte MUST be 0.
// That is, (byte0 & 0xC0) == 0. RFC 5389 §6.
//
// Attribute layout:
//   bytes  0..2  Attribute Type (u16)
//   bytes  2..4  Attribute Length (u16)
//   bytes  4..n  Attribute Value (padded to a 4-byte boundary)
//
// Supported attributes (per RFC 5389):
//   0x0001  MAPPED-ADDRESS       (IPv4 / IPv6, family byte)
//   0x0020  XOR-MAPPED-ADDRESS   (XOR port = port ^ 0x2112; IPv4 addr = addr ^ magic cookie)
//   0x8022  SOFTWARE             (UTF-8)
//   0x8028  FINGERPRINT          (CRC32 over the STUN message up to but not including the attr)

use std::fmt;

/// STUN magic cookie from RFC 5389 §6.
pub const MAGIC_COOKIE: u32 = 0x2112_A442;

/// Total header length in bytes.
pub const HEADER_LEN: usize = 20;

// Common message types (first two bytes; method + class).
pub const BINDING_REQUEST: u16 = 0x0001;
pub const BINDING_RESPONSE: u16 = 0x0101;
pub const BINDING_ERROR: u16 = 0x0111;

/// Address families used by MAPPED-ADDRESS / XOR-MAPPED-ADDRESS.
pub const FAMILY_IPV4: u8 = 0x01;
pub const FAMILY_IPV6: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedAddress {
    pub family: u8,
    pub port: u16,
    pub addr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    MappedAddress(MappedAddress),
    XorMappedAddress(MappedAddress),
    Software(String),
    Fingerprint(u32),
    Other { ty: u16, value: Vec<u8> },
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Attribute::MappedAddress(m) => write!(
                f,
                "MAPPED-ADDRESS family={} port={} addr={}",
                m.family,
                m.port,
                hex_bytes(&m.addr)
            ),
            Attribute::XorMappedAddress(m) => write!(
                f,
                "XOR-MAPPED-ADDRESS family={} port={} addr={}",
                m.family,
                m.port,
                hex_bytes(&m.addr)
            ),
            Attribute::Software(s) => write!(f, "SOFTWARE {}", s),
            Attribute::Fingerprint(c) => write!(f, "FINGERPRINT 0x{:08x}", c),
            Attribute::Other { ty, value } => write!(
                f,
                "ATTR 0x{:04x} len={}",
                ty,
                value.len()
            ),
        }
    }
}

fn hex_bytes(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 3);
    for (i, byte) in b.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub msg_type: u16,
    pub msg_len: u16,
    pub magic_cookie: u32,
    pub transaction_id: [u8; 12],
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooShort,
    BadMagicCookie(u32),
    TruncatedAttribute,
    BadAttributeLength,
    InvalidUtf8,
    UnknownFamily(u8),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort => write!(f, "input shorter than 20-byte STUN header"),
            Error::BadMagicCookie(c) => write!(f, "bad STUN magic cookie: 0x{:08x}", c),
            Error::TruncatedAttribute => write!(f, "STUN attribute truncated"),
            Error::BadAttributeLength => write!(f, "STUN attribute length exceeds message"),
            Error::InvalidUtf8 => write!(f, "SOFTWARE attribute is not valid UTF-8"),
            Error::UnknownFamily(fam) => write!(f, "unknown address family: {}", fam),
        }
    }
}

/// Detect whether the leading bytes look like a STUN message per RFC 5389 §6.
///
/// The first two bits of byte 0 must be 0; the magic cookie at bytes 4..8 must be
/// `0x2112A442`.
pub fn is_stun(buf: &[u8]) -> bool {
    if buf.len() < HEADER_LEN {
        return false;
    }
    if buf[0] & 0xC0 != 0 {
        return false;
    }
    let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    cookie == MAGIC_COOKIE
}

/// Parse a complete STUN message from `buf`. Returns [`Error::TooShort`] when the
/// buffer is less than 20 bytes, [`Error::BadMagicCookie`] when the magic cookie
/// doesn't match.
pub fn parse(buf: &[u8]) -> Result<Message, Error> {
    if buf.len() < HEADER_LEN {
        return Err(Error::TooShort);
    }
    if buf[0] & 0xC0 != 0 {
        // Reserved bits set → not STUN.
        return Err(Error::BadMagicCookie(0));
    }
    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
    let msg_len = u16::from_be_bytes([buf[2], buf[3]]);
    let magic_cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if magic_cookie != MAGIC_COOKIE {
        return Err(Error::BadMagicCookie(magic_cookie));
    }
    let mut transaction_id = [0u8; 12];
    transaction_id.copy_from_slice(&buf[8..20]);

    let payload_end = HEADER_LEN
        .checked_add(msg_len as usize)
        .ok_or(Error::BadAttributeLength)?;
    if buf.len() < payload_end {
        return Err(Error::BadAttributeLength);
    }
    let payload = &buf[HEADER_LEN..payload_end];

    let mut attributes = Vec::new();
    let mut i = 0;
    while i < payload.len() {
        let rem = &payload[i..];
        if rem.len() < 4 {
            return Err(Error::TruncatedAttribute);
        }
        let ty = u16::from_be_bytes([rem[0], rem[1]]);
        let len = u16::from_be_bytes([rem[2], rem[3]]) as usize;
        if rem.len() < 4 + len {
            return Err(Error::BadAttributeLength);
        }
        let value = &rem[4..4 + len];
        attributes.push(parse_attribute(ty, value, magic_cookie, transaction_id)?);
        // Pad up to 4-byte boundary per RFC 5389 §15.
        let padded = (len + 3) & !3;
        i += 4 + padded;
    }

    Ok(Message {
        msg_type,
        msg_len,
        magic_cookie,
        transaction_id,
        attributes,
    })
}

fn parse_attribute(
    ty: u16,
    value: &[u8],
    magic_cookie: u32,
    transaction_id: [u8; 12],
) -> Result<Attribute, Error> {
    match ty {
        0x0001 => Ok(Attribute::MappedAddress(parse_mapped_address(value)?)),
        0x0020 => Ok(Attribute::XorMappedAddress(parse_xor_mapped_address(
            value,
            magic_cookie,
            &transaction_id,
        )?)),
        0x8022 => {
            let s = std::str::from_utf8(value)
                .map_err(|_| Error::InvalidUtf8)?
                .to_string();
            Ok(Attribute::Software(s))
        }
        0x8028 => {
            if value.len() != 4 {
                return Err(Error::BadAttributeLength);
            }
            let crc = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
            Ok(Attribute::Fingerprint(crc))
        }
        _ => Ok(Attribute::Other {
            ty,
            value: value.to_vec(),
        }),
    }
}

/// Parse MAPPED-ADDRESS value: 1 byte reserved (0), 1 byte family, 2 bytes port,
/// 4 bytes IPv4 or 16 bytes IPv6.
fn parse_mapped_address(value: &[u8]) -> Result<MappedAddress, Error> {
    if value.len() < 4 {
        return Err(Error::TruncatedAttribute);
    }
    let family = value[1];
    let port = u16::from_be_bytes([value[2], value[3]]);
    let addr = match family {
        FAMILY_IPV4 => {
            if value.len() < 8 {
                return Err(Error::TruncatedAttribute);
            }
            value[4..8].to_vec()
        }
        FAMILY_IPV6 => {
            if value.len() < 20 {
                return Err(Error::TruncatedAttribute);
            }
            value[4..20].to_vec()
        }
        other => return Err(Error::UnknownFamily(other)),
    };
    Ok(MappedAddress { family, port, addr })
}

/// Parse XOR-MAPPED-ADDRESS: same layout as MAPPED-ADDRESS but the port is XORed
/// with the top 16 bits of the magic cookie (`0x2112`) and the IPv4 address is
/// XORed with the magic cookie. IPv6 additionally mixes the transaction ID per
/// RFC 5389 §15.2.
fn parse_xor_mapped_address(
    value: &[u8],
    magic_cookie: u32,
    transaction_id: &[u8; 12],
) -> Result<MappedAddress, Error> {
    if value.len() < 4 {
        return Err(Error::TruncatedAttribute);
    }
    let family = value[1];
    let xport = u16::from_be_bytes([value[2], value[3]]);
    let port = xport ^ (MAGIC_COOKIE >> 16) as u16;

    let addr = match family {
        FAMILY_IPV4 => {
            if value.len() < 8 {
                return Err(Error::TruncatedAttribute);
            }
            let x = u32::from_be_bytes([value[4], value[5], value[6], value[7]]);
            (x ^ magic_cookie).to_be_bytes().to_vec()
        }
        FAMILY_IPV6 => {
            if value.len() < 20 {
                return Err(Error::TruncatedAttribute);
            }
            let mc_bytes = magic_cookie.to_be_bytes();
            let mut out = [0u8; 16];
            for j in 0..4 {
                out[j] = value[4 + j] ^ mc_bytes[j];
            }
            for j in 0..12 {
                out[4 + j] = value[8 + j] ^ transaction_id[j];
            }
            out.to_vec()
        }
        other => return Err(Error::UnknownFamily(other)),
    };
    Ok(MappedAddress { family, port, addr })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding_request_with_attr(attr_ty: u16, attr_val: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        // Type 0x0001 (Binding Request), length placeholder.
        buf.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
        let attr_len = attr_val.len() as u16;
        buf.extend_from_slice(&attr_len.to_be_bytes());
        buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        let txid = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        buf.extend_from_slice(&txid);
        buf.extend_from_slice(&attr_ty.to_be_bytes());
        buf.extend_from_slice(&attr_len.to_be_bytes());
        buf.extend_from_slice(attr_val);
        // Pad to 4 bytes.
        let pad = (4 - (attr_val.len() % 4)) % 4;
        buf.extend(std::iter::repeat(0u8).take(pad));
        // First-pass header had wrong length; rewrite.
        let total_payload_len = (buf.len() - HEADER_LEN) as u16;
        buf[2..4].copy_from_slice(&total_payload_len.to_be_bytes());
        buf
    }

    #[test]
    fn detects_valid_stun_header() {
        let pkt = binding_request_with_attr(0x8022, b"test");
        assert!(is_stun(&pkt));
    }

    #[test]
    fn rejects_non_stun_first_byte() {
        let mut pkt = binding_request_with_attr(0x8022, b"x");
        // Set the two reserved bits to non-zero (RFC 5389 §6).
        pkt[0] |= 0x80;
        assert!(!is_stun(&pkt));
        let err = parse(&pkt).unwrap_err();
        assert_eq!(err, Error::BadMagicCookie(0));
    }

    #[test]
    fn rejects_short_input() {
        assert!(!is_stun(&[0u8; 5]));
        assert_eq!(parse(&[]).unwrap_err(), Error::TooShort);
        assert_eq!(parse(&[0u8; 10]).unwrap_err(), Error::TooShort);
    }

    #[test]
    fn parses_binding_request_header() {
        let pkt = binding_request_with_attr(0x8022, b"hello");
        let msg = parse(&pkt).expect("parse ok");
        assert_eq!(msg.msg_type, BINDING_REQUEST);
        assert_eq!(msg.magic_cookie, MAGIC_COOKIE);
        assert_eq!(msg.transaction_id, [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(msg.attributes.len(), 1);
        match &msg.attributes[0] {
            Attribute::Software(s) => assert_eq!(s, "hello"),
            other => panic!("expected SOFTWARE got {:?}", other),
        }
    }

    #[test]
    fn parses_mapped_address_ipv4() {
        // Reserved(0) + family(0x01) + port(0x1234) + addr(10.0.0.1)
        let mut val = vec![0x00, FAMILY_IPV4];
        val.extend_from_slice(&0x1234_u16.to_be_bytes());
        val.extend_from_slice(&[10, 0, 0, 1]);
        let pkt = binding_request_with_attr(0x0001, &val);
        let msg = parse(&pkt).unwrap();
        match &msg.attributes[0] {
            Attribute::MappedAddress(m) => {
                assert_eq!(m.family, FAMILY_IPV4);
                assert_eq!(m.port, 0x1234);
                assert_eq!(m.addr, vec![10, 0, 0, 1]);
            }
            other => panic!("expected MAPPED-ADDRESS got {:?}", other),
        }
    }

    #[test]
    fn parses_xor_mapped_address_ipv4() {
        // Real IPv4 192.0.2.1:3478 → XOR with magic cookie.
        let real_port: u16 = 3478;
        let real_addr: [u8; 4] = [192, 0, 2, 1];
        let xor_port = real_port ^ (MAGIC_COOKIE >> 16) as u16;
        let xor_addr: [u8; 4] = {
            let v = u32::from_be_bytes(real_addr) ^ MAGIC_COOKIE;
            v.to_be_bytes()
        };
        let mut val = vec![0x00, FAMILY_IPV4];
        val.extend_from_slice(&xor_port.to_be_bytes());
        val.extend_from_slice(&xor_addr);
        let pkt = binding_request_with_attr(0x0020, &val);
        let msg = parse(&pkt).unwrap();
        match &msg.attributes[0] {
            Attribute::XorMappedAddress(m) => {
                assert_eq!(m.family, FAMILY_IPV4);
                assert_eq!(m.port, real_port);
                assert_eq!(m.addr, real_addr.to_vec());
            }
            other => panic!("expected XOR-MAPPED-ADDRESS got {:?}", other),
        }
    }

    #[test]
    fn parses_fingerprint() {
        let crc = 0x1234_5678_u32;
        let pkt = binding_request_with_attr(0x8028, &crc.to_be_bytes());
        let msg = parse(&pkt).unwrap();
        match &msg.attributes[0] {
            Attribute::Fingerprint(c) => assert_eq!(*c, crc),
            other => panic!("expected FINGERPRINT got {:?}", other),
        }
    }

    #[test]
    fn rejects_bad_magic_cookie() {
        let mut pkt = binding_request_with_attr(0x8022, b"x");
        // Smash the magic cookie.
        pkt[4..8].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let err = parse(&pkt).unwrap_err();
        assert_eq!(err, Error::BadMagicCookie(0xDEADBEEF));
    }

    #[test]
    fn rejects_truncated_attribute_value() {
        // Build a packet whose msg_len claims an attribute larger than the buffer.
        let mut buf = Vec::new();
        buf.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
        buf.extend_from_slice(&20u16.to_be_bytes()); // length = 20 bytes
        buf.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        let txid = [0u8; 12];
        buf.extend_from_slice(&txid);
        // Attribute says its value is 16 bytes but only provide 4.
        buf.extend_from_slice(&0x8022_u16.to_be_bytes());
        buf.extend_from_slice(&16u16.to_be_bytes());
        buf.extend_from_slice(b"hi!!");
        let err = parse(&buf).unwrap_err();
        assert_eq!(err, Error::BadAttributeLength);
    }

    #[test]
    fn display_format_does_not_panic() {
        let pkt = binding_request_with_attr(0x8022, b"substrate-stun");
        let msg = parse(&pkt).unwrap();
        let _ = format!("{}", msg.attributes[0]);
    }
}