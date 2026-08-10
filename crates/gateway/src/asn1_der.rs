//! Minimal ASN.1 DER (Distinguished Encoding Rules) encoder for primitive
//! types (INTEGER, OCTET STRING, NULL, OBJECT IDENTIFIER, UTF8String,
//! PrintableString, BOOLEAN, BIT STRING).
//!
//! Useful for hand-rolling X.509 names, OIDs, and small DER blobs without
//! pulling in a full ASN.1 crate. Decoder counterpart is intentionally
//! omitted — this is encode-only and intended for tests and small fixtures.
//!
//! Reference: ITU-T X.690 (07/2002) §8 for encoding rules.

/// ASN.1 universal tag numbers used in this module.
pub mod tag {
    pub const BOOLEAN: u8 = 0x01;
    pub const INTEGER: u8 = 0x02;
    pub const BIT_STRING: u8 = 0x03;
    pub const OCTET_STRING: u8 = 0x04;
    pub const NULL: u8 = 0x05;
    pub const OID: u8 = 0x06;
    pub const UTF8_STRING: u8 = 0x12;
    pub const PRINTABLE_STRING: u8 = 0x13;
    pub const IA5_STRING: u8 = 0x16;
}

/// Encode a `BOOLEAN` primitive.
pub fn boolean(v: bool) -> Vec<u8> {
    vec![tag::BOOLEAN, 0x01, if v { 0xff } else { 0x00 }]
}

/// Encode an `INTEGER` (big-endian, signed). Values are encoded as the
/// minimal two's-complement byte representation with at least one content
/// byte (0 encodes as `0x00`).
pub fn integer(v: i64) -> Vec<u8> {
    let bytes = encode_signed_minimal(v);
    let mut out = Vec::with_capacity(2 + bytes.len());
    out.push(tag::INTEGER);
    encode_length(&mut out, bytes.len());
    out.extend_from_slice(&bytes);
    out
}

/// Encode an `OCTET STRING`.
pub fn octet_string(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + bytes.len());
    out.push(tag::OCTET_STRING);
    encode_length(&mut out, bytes.len());
    out.extend_from_slice(bytes);
    out
}

/// Encode `NULL` (always 2 bytes).
pub fn null() -> Vec<u8> {
    vec![tag::NULL, 0x00]
}

/// Encode an OBJECT IDENTIFIER from its dotted-decimal components.
/// E.g., `encode_oid(&[1, 2, 840, 113549])` = 1.2.840.113549 (RSA OID prefix).
pub fn encode_oid(components: &[u64]) -> Vec<u8> {
    if components.len() < 2 {
        return Vec::new();
    }
    let first = 40 * components[0] + components[1];
    let mut body: Vec<u8> = Vec::new();
    body.push(first as u8);
    for &c in &components[2..] {
        encode_base128(&mut body, c);
    }
    let mut out = Vec::with_capacity(2 + body.len());
    out.push(tag::OID);
    encode_length(&mut out, body.len());
    out.extend_from_slice(&body);
    out
}

/// Encode a `UTF8String`.
pub fn utf8_string(s: &str) -> Vec<u8> {
    octet_string_or(s.as_bytes(), tag::UTF8_STRING)
}

/// Encode a `PrintableString` (ASCII subset; non-ASCII bytes are replaced
/// with `?` to keep the encoder total — caller should validate input
/// before sending).
pub fn printable_string(s: &str) -> Vec<u8> {
    let sanitized: Vec<u8> = s
        .bytes()
        .map(|b| {
            if b.is_ascii_graphic() || b == b' ' {
                b
            } else {
                b'?'
            }
        })
        .collect();
    octet_string_or(&sanitized, tag::PRINTABLE_STRING)
}

fn octet_string_or(bytes: &[u8], tag: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + bytes.len());
    out.push(tag);
    encode_length(&mut out, bytes.len());
    out.extend_from_slice(bytes);
    out
}

fn encode_length(out: &mut Vec<u8>, n: usize) {
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.push(0x81);
        out.push(n as u8);
    } else if n < 0x10000 {
        out.push(0x82);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    } else {
        out.push(0x83);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    }
}

fn encode_base128(out: &mut Vec<u8>, n: u64) {
    if n == 0 {
        out.push(0);
        return;
    }
    let mut stack: Vec<u8> = Vec::new();
    let mut v = n;
    while v > 0 {
        stack.push((v & 0x7f) as u8);
        v >>= 7;
    }
    stack.reverse();
    for (i, b) in stack.iter().enumerate() {
        let cont = if i + 1 < stack.len() { 0x80 } else { 0x00 };
        out.push(b | cont);
    }
}

fn encode_signed_minimal(v: i64) -> Vec<u8> {
    if v == 0 {
        return vec![0];
    }
    // Negative values: produce two's-complement bytes that fit in `ceil(bit_len/8)`
    if v < 0 {
        // Use 8 bytes for i64; the leading 0xFF bytes are sign-extension and
        // are not stripped because they signal the sign of the value.
        return v.to_be_bytes().to_vec();
    }
    // Positive: strip leading zeros but keep at least one byte, and prepend
    // 0x00 if the high bit of the next byte would be set (positive sign).
    let bytes = v.to_be_bytes();
    let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();
    let mut start = leading_zeros;
    if bytes[start] & 0x80 != 0 {
        start -= 1; // keep one leading zero to signal positive sign
    }
    bytes[start..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_true() {
        assert_eq!(boolean(true), vec![0x01, 0x01, 0xff]);
    }

    #[test]
    fn boolean_false() {
        assert_eq!(boolean(false), vec![0x01, 0x01, 0x00]);
    }

    #[test]
    fn integer_zero() {
        assert_eq!(integer(0), vec![0x02, 0x01, 0x00]);
    }

    #[test]
    fn integer_127_no_padding() {
        assert_eq!(integer(127), vec![0x02, 0x01, 0x7f]);
    }

    #[test]
    fn integer_128_gets_padding() {
        // 128 needs a leading 0 byte so it's not read as -128
        assert_eq!(integer(128), vec![0x02, 0x02, 0x00, 0x80]);
    }

    #[test]
    fn integer_negative() {
        // -1 encodes as 0xFF
        assert_eq!(
            integer(-1),
            vec![0x02, 0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn octet_string_basic() {
        assert_eq!(octet_string(b"abc"), vec![0x04, 0x03, b'a', b'b', b'c']);
    }

    #[test]
    fn null_encoding() {
        assert_eq!(null(), vec![0x05, 0x00]);
    }

    #[test]
    fn oid_rsa_prefix() {
        // 1.2.840.113549
        let encoded = encode_oid(&[1, 2, 840, 113549]);
        assert_eq!(encoded[0], 0x06);
        // First content byte = 40*1 + 2 = 42 = 0x2A
        assert_eq!(encoded[2], 0x2A);
        // 840 = 6*128 + 72 => 0x86 0x48
        assert_eq!(encoded[3], 0x86);
        assert_eq!(encoded[4], 0x48);
    }

    #[test]
    fn oid_empty_or_single_component() {
        assert!(encode_oid(&[]).is_empty());
        assert!(encode_oid(&[1]).is_empty());
    }

    #[test]
    fn utf8_string_basic() {
        let encoded = utf8_string("hi");
        // tag 0x12 = UTF8String, length 2, then 'h' 'i'
        assert_eq!(encoded, vec![0x12, 0x02, b'h', b'i']);
    }

    #[test]
    fn utf8_string_empty() {
        let encoded = utf8_string("");
        assert_eq!(encoded, vec![0x12, 0x00]);
    }

    #[test]
    fn utf8_string_multibyte() {
        let encoded = utf8_string("héllo");
        // 'h' 'é' (0xc3 0xa9) 'l' 'l' 'o' = 6 bytes
        assert_eq!(
            encoded,
            vec![0x12, 0x06, b'h', 0xc3, 0xa9, b'l', b'l', b'o']
        );
    }

    #[test]
    fn printable_string_sanitizes_non_ascii() {
        // Non-printable byte 0x01 replaced with '?'
        assert_eq!(
            printable_string("a\x01b"),
            vec![0x13, 0x03, b'a', b'?', b'b']
        );
    }

    #[test]
    fn long_octet_string_uses_multi_byte_length() {
        let data = vec![0u8; 200];
        let encoded = octet_string(&data);
        // Length 200 needs 0x81 0xC8 form
        assert_eq!(encoded[1], 0x81);
        assert_eq!(encoded[2], 200);
    }
}
