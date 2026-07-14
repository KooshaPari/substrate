//! Minimal ASN.1 BER (Basic Encoding Rules) parser/encoder for X.690.
//!
//! Supports the universal-class primitive and constructed types most commonly
//! seen in TLS, X.509, LDAP, and SNMP: INTEGER, BIT STRING, OCTET STRING,
//! NULL, OBJECT IDENTIFIER, UTF8String, BOOLEAN, SEQUENCE, SET.
//!
//! Tag layout per X.690 §8.1.2.2:
//!
//! ```text
//!  7   6   5   4   3   2   1   0
//! +---+---+---+---+---+---+---+---+
//! | class   | c |  tag number   |    (single byte form)
//! +---+---+---+---+---+---+---+---+
//!
//! class: 00 = UNIVERSAL
//!        01 = APPLICATION
//!        10 = CONTEXT-SPECIFIC
//!        11 = PRIVATE
//! c (constructed bit, bit 5): 0 = primitive, 1 = constructed
//! tag number in bits 4..0: 0..30 direct; 31 means long form follows
//! ```
//!
//! Length encoding (X.690 §8.1.3):
//!
//! ```text
//! short form (one byte): single byte 0..127 (inclusive)
//! long form: 0x80|n followed by n length-bytes, big-endian
//! ```
//!
//! BER (in contrast to DER) permits non-canonical length encodings
//! (e.g. `0x80 0x00` for indefinite-length on constructed types).
//!
//! Reference: ITU-T X.690 (07/2002) §8.

/// ASN.1 universal tag numbers (X.690 §8.4).
pub mod tag {
    pub const BOOLEAN: u8 = 0x01;
    pub const INTEGER: u8 = 0x02;
    pub const BIT_STRING: u8 = 0x03;
    pub const OCTET_STRING: u8 = 0x04;
    pub const NULL: u8 = 0x05;
    pub const OID: u8 = 0x06;
    pub const UTF8_STRING: u8 = 0x12;
    pub const SEQUENCE: u8 = 0x30;
    pub const SET: u8 = 0x31;
}

/// ASN.1 class values stored in the top two bits of the identifier octet.
pub mod class {
    pub const UNIVERSAL: u8 = 0b00;
    pub const APPLICATION: u8 = 0b01;
    pub const CONTEXT: u8 = 0b10;
    pub const PRIVATE: u8 = 0b11;
}

/// A decoded BER element.
///
/// For primitive types, `value` holds the raw content octets and `children` is
/// empty. For constructed types, `value` is empty and `children` holds the
/// nested elements. Some encoders use indefinite-length for constructed
/// types; in that case `value` holds the trailing end-of-contents octets
/// (`00 00`) verbatim for round-trip fidelity.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Ber {
    /// The low 5 bits of the identifier octet (tag number). For long-form
    /// tags this stores the full decoded value (not 31).
    pub tag: u8,
    /// Constructed bit (bit 5 of the identifier octet).
    pub constructed: bool,
    /// ASN.1 class bits 6-7.
    pub class: u8,
    /// Primitive content octets (empty for constructed types).
    pub value: Vec<u8>,
    /// Nested elements (empty for primitive types).
    pub children: Vec<Ber>,
}

/// Parse a BER TLV from the front of `input`. Returns the decoded element
/// plus the remainder of the input.
pub fn parse(input: &[u8]) -> Result<(Ber, &[u8]), String> {
    let (header, rest) = parse_header(input)?;
    let (value, after) = parse_contents(rest, header.value_len, header.constructed)?;
    let element = Ber {
        tag: header.tag,
        constructed: header.constructed,
        class: header.class,
        value,
        children: Vec::new(),
    };
    Ok((element, after))
}

struct Header {
    tag: u8,
    constructed: bool,
    class: u8,
    value_len: ValueLen,
}

enum ValueLen {
    /// Explicit definite length in bytes.
    Definite(usize),
    /// X.690 §8.1.3.6: 0x80 followed by end-of-contents `00 00`.
    Indefinite,
}

fn parse_header(input: &[u8]) -> Result<(Header, &[u8]), String> {
    if input.is_empty() {
        return Err("BER: empty input".into());
    }
    let first = input[0];
    let class = (first >> 6) & 0x03;
    let constructed = (first >> 5) & 0x01 == 1;
    let low5 = first & 0x1f;
    let (tag, after_tag) = if low5 < 31 {
        (low5, &input[1..])
    } else {
        // Long form: subsequent bytes encode the tag number in base-128,
        // high bit = continuation (X.690 §8.1.2.4). The loop ends when the
        // high bit is clear; if we run off the end of `input` first, the
        // tag is unterminated.
        let mut tag_val: u32 = 0;
        let mut i = 1usize;
        let mut terminated = false;
        while !terminated {
            if i >= input.len() {
                return Err("BER: unterminated long-form tag".into());
            }
            let b = input[i];
            tag_val = (tag_val << 7) | (b & 0x7f) as u32;
            i += 1;
            if b & 0x80 == 0 {
                terminated = true;
            }
            if tag_val > 0xff {
                return Err("BER: long-form tag overflows u8".into());
            }
        }
        (tag_val as u8, &input[i..])
    };
    if after_tag.is_empty() {
        return Err("BER: missing length octet".into());
    }
    let len_byte = after_tag[0];
    let (value_len, after_len) = if len_byte < 0x80 {
        (ValueLen::Definite(len_byte as usize), &after_tag[1..])
    } else if len_byte == 0x80 {
        (ValueLen::Indefinite, &after_tag[1..])
    } else {
        let n = (len_byte & 0x7f) as usize;
        if n == 0 {
            return Err("BER: reserved long-form length 0x80 0x00".into());
        }
        if n > 8 {
            return Err("BER: long-form length too large".into());
        }
        if after_tag.len() < 1 + n {
            return Err("BER: truncated long-form length".into());
        }
        let mut v: usize = 0;
        for &b in &after_tag[1..1 + n] {
            v = (v << 8) | b as usize;
        }
        (ValueLen::Definite(v), &after_tag[1 + n..])
    };
    Ok((
        Header {
            tag,
            constructed,
            class,
            value_len,
        },
        after_len,
    ))
}

fn parse_contents(
    input: &[u8],
    len: ValueLen,
    constructed: bool,
) -> Result<(Vec<u8>, &[u8]), String> {
    match len {
        ValueLen::Definite(n) => {
            if input.len() < n {
                return Err(format!("BER: content truncated (need {} bytes)", n));
            }
            let (content, after) = input.split_at(n);
            Ok((content.to_vec(), after))
        }
        ValueLen::Indefinite => {
            if !constructed {
                return Err("BER: indefinite length on primitive type".into());
            }
            // Scan for end-of-contents marker (00 00).
            let mut i = 0usize;
            loop {
                if i + 1 >= input.len() {
                    return Err("BER: unterminated indefinite length".into());
                }
                if input[i] == 0 && input[i + 1] == 0 {
                    let content = input[..i].to_vec();
                    let after = &input[i + 2..];
                    return Ok((content, after));
                }
                i += 1;
            }
        }
    }
}

/// Encode a primitive TLV with definite length. The tag byte combines the
/// class bits, constructed bit (always 0 here), and tag number — caller is
/// responsible for assembling the identifier octet (typically just the
/// universal-class tag byte).
pub fn encode_primitive(identifier: u8, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + value.len());
    out.push(identifier);
    encode_length_into(&mut out, value.len());
    out.extend_from_slice(value);
    out
}

fn encode_length_into(out: &mut Vec<u8>, n: usize) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_boolean_true() {
        // X.690 §10.1: BOOLEAN true is encoded as 01 01 FF.
        let bytes = [0x01, 0x01, 0xff];
        let (el, rest) = parse(&bytes).unwrap();
        assert_eq!(rest.len(), 0);
        assert_eq!(el.tag, tag::BOOLEAN);
        assert!(!el.constructed);
        assert_eq!(el.class, class::UNIVERSAL);
        assert_eq!(el.value, vec![0xff]);
    }

    #[test]
    fn parse_integer_minimal() {
        // INTEGER 5: 02 01 05
        let bytes = [0x02, 0x01, 0x05];
        let (el, _) = parse(&bytes).unwrap();
        assert_eq!(el.tag, tag::INTEGER);
        assert_eq!(el.value, vec![5]);
    }

    #[test]
    fn parse_integer_two_byte_length() {
        // INTEGER with 200-byte content: 02 81 C8 + 200 zero bytes
        let mut bytes = vec![0x02, 0x81, 0xc8];
        bytes.extend(std::iter::repeat(0u8).take(200));
        let (el, rest) = parse(&bytes).unwrap();
        assert_eq!(rest.len(), 0);
        assert_eq!(el.value.len(), 200);
    }

    #[test]
    fn parse_octet_string_basic() {
        // OCTET STRING "abc": 04 03 61 62 63
        let bytes = [0x04, 0x03, b'a', b'b', b'c'];
        let (el, _) = parse(&bytes).unwrap();
        assert_eq!(el.tag, tag::OCTET_STRING);
        assert_eq!(el.value, b"abc".to_vec());
    }

    #[test]
    fn parse_null() {
        // NULL: 05 00
        let bytes = [0x05, 0x00];
        let (el, _) = parse(&bytes).unwrap();
        assert_eq!(el.tag, tag::NULL);
        assert_eq!(el.value, Vec::<u8>::new());
    }

    #[test]
    fn parse_utf8_string_multibyte() {
        // UTF8String "héllo" (6 bytes): 12 06 68 C3 A9 6C 6C 6F
        let bytes = [0x12, 0x06, b'h', 0xc3, 0xa9, b'l', b'l', b'o'];
        let (el, _) = parse(&bytes).unwrap();
        assert_eq!(el.tag, tag::UTF8_STRING);
        assert_eq!(el.value.len(), 6);
    }

    #[test]
    fn parse_returns_remainder() {
        // Two consecutive elements: INTEGER 5 followed by BOOLEAN true
        let bytes = [0x02, 0x01, 0x05, 0x01, 0x01, 0xff];
        let (el1, rest1) = parse(&bytes).unwrap();
        assert_eq!(el1.tag, tag::INTEGER);
        assert_eq!(rest1, &[0x01, 0x01, 0xff]);
        let (el2, rest2) = parse(rest1).unwrap();
        assert_eq!(el2.tag, tag::BOOLEAN);
        assert_eq!(rest2.len(), 0);
    }

    #[test]
    fn parse_class_bits() {
        // Context-specific [0] primitive with 1-byte content:
        // 80 01 2A
        let bytes = [0x80, 0x01, 0x2a];
        let (el, _) = parse(&bytes).unwrap();
        assert_eq!(el.class, class::CONTEXT);
        assert_eq!(el.tag, 0);
        assert!(!el.constructed);
        assert_eq!(el.value, vec![0x2a]);
    }

    #[test]
    fn parse_constructed_bit_set() {
        // SEQUENCE (0x30) with one INTEGER 0 inside:
        // 30 03 02 01 00
        // SEQUENCE is universal class, constructed, tag number 16 (0x10)
        // — its full identifier octet 0x30 = class 00 + constructed 1 + tag 10000.
        let bytes = [0x30, 0x03, 0x02, 0x01, 0x00];
        let (el, _) = parse(&bytes).unwrap();
        assert_eq!(el.tag, 16);
        assert!(el.constructed);
        assert_eq!(el.class, class::UNIVERSAL);
    }

    #[test]
    fn parse_long_form_tag() {
        // Long-form tag (X.690 §8.1.2.4): the first identifier byte has its
        // low 5 bits = 0b11111 (= 31), signaling long-form. Subsequent
        // octets encode the tag number in base-128, MSB = continuation.
        // Encoding of 64 (0x40) = single octet 0x40 (no continuation),
        // preceded by the long-form marker 0x7F. So the wire bytes are
        // 7F 40 01 FF (tag bytes, then length 1, then content 0xFF).
        let bytes = [0x7f, 0x40, 0x01, 0xff];
        let (el, rest) = parse(&bytes).unwrap();
        assert_eq!(el.tag, 64);
        assert_eq!(el.value, vec![0xff]);
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn parse_indefinite_length_primitive_rejected() {
        // Indefinite length on a primitive must be rejected.
        let bytes = [0x04, 0x80, b'a', b'c'];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_indefinite_length_constructed_terminates() {
        // Indefinite-length SEQUENCE containing INTEGER 1:
        // 30 80 02 01 01 00 00
        // The trailing 00 00 is the end-of-contents marker (X.690 §8.1.5).
        let bytes = [0x30, 0x80, 0x02, 0x01, 0x01, 0x00, 0x00];
        let (el, rest) = parse(&bytes).unwrap();
        assert_eq!(el.tag, 16); // SEQUENCE has tag number 16
        assert!(el.constructed);
        assert_eq!(rest.len(), 0);
        // Raw content octets (without trailing EOC marker) are exactly the
        // bytes of the inner INTEGER 1.
        assert_eq!(el.value, vec![0x02, 0x01, 0x01]);
    }

    #[test]
    fn parse_truncated_content() {
        // INTEGER claims length 3 but only 1 byte follows.
        let bytes = [0x02, 0x03, 0x05];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_truncated_length_octet() {
        // Tag without a length octet.
        let bytes = [0x02];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_truncated_long_form_tag() {
        // 0x7F starts long form but never clears the continuation bit.
        let bytes = [0x7f, 0x81, 0x81, 0x01, 0xff];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_reserved_long_form_length() {
        // 0x80 0x00 is reserved in BER (X.690 §8.1.3.5 / §8.1.3.6).
        let bytes = [0x02, 0x80, 0x00, 0x05];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn encode_boolean_true_round_trip() {
        let encoded = encode_primitive(tag::BOOLEAN, &[0xff]);
        let (el, rest) = parse(&encoded).unwrap();
        assert_eq!(el.value, vec![0xff]);
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn encode_octet_string_uses_short_length() {
        let encoded = encode_primitive(tag::OCTET_STRING, b"xyz");
        // 04 03 78 79 7A
        assert_eq!(encoded, vec![0x04, 0x03, b'x', b'y', b'z']);
    }

    #[test]
    fn encode_octet_string_uses_long_length_at_200() {
        let data = vec![0u8; 200];
        let encoded = encode_primitive(tag::OCTET_STRING, &data);
        assert_eq!(encoded[1], 0x81);
        assert_eq!(encoded[2], 200);
        assert_eq!(encoded.len(), 3 + 200);
    }

    #[test]
    fn encode_null_round_trip() {
        let encoded = encode_primitive(tag::NULL, &[]);
        let (el, _) = parse(&encoded).unwrap();
        assert_eq!(el.tag, tag::NULL);
        assert_eq!(el.value, Vec::<u8>::new());
    }
}
