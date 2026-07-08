//! Minimal DHCPv6 message parser/encoder (RFC 8415).
//!
//! A DHCPv6 message has the format:
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |    msg-type   |               transaction-id                  |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! .                                                               .
//! .            options (variable number and length)               .
//! .                                                               .
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```
//!
//! Each option has the format:
//!
//! ```text
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |        option-code            |           option-len          |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                                                               |
//! .                          option-data                          .
//! .                                                               .
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```
//!
//! Reference: RFC 8415 §8 (Message Formats) and §21 (Option Formats).
//! Top-level message format: RFC 8415 §8.1.

/// DHCPv6 message types (RFC 8415 §8.1 / IANA DHCPv6 Parameters registry).
pub mod msg_type {
    pub const SOLICIT: u8 = 1;
    pub const ADVERTISE: u8 = 2;
    pub const REQUEST: u8 = 3;
    pub const CONFIRM: u8 = 4;
    pub const RENEW: u8 = 5;
    pub const REBIND: u8 = 6;
    pub const REPLY: u8 = 7;
    pub const RELEASE: u8 = 8;
    pub const DECLINE: u8 = 9;
    pub const RECONFIGURE: u8 = 10;
    pub const INFORMATION_REQUEST: u8 = 11;
    pub const RELAY_FORWARD: u8 = 12;
    pub const RELAY_REPLY: u8 = 13;
}

/// Common DHCPv6 option codes (RFC 8415 §21 / IANA registry).
pub mod option_code {
    pub const CLIENTID: u16 = 1;
    pub const SERVERID: u16 = 2;
    pub const IA_NA: u16 = 3;
    pub const IA_TA: u16 = 4;
    pub const IAADDR: u16 = 5;
    pub const ORO: u16 = 6;
    pub const PREFERENCE: u16 = 7;
    pub const ELAPSED_TIME: u16 = 8;
    pub const RELAY_MSG: u16 = 9;
    pub const DNS_SERVERS: u16 = 23;
    pub const DOMAIN_LIST: u16 = 24;
    pub const IA_PD: u16 = 25;
    pub const IAPREFIX: u16 = 26;
}

/// A decoded DHCPv6 option.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Dhcp6Option {
    pub code: u16,
    pub value: Vec<u8>,
}

/// A decoded DHCPv6 message.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Dhcp6Msg {
    pub msg_type: u8,
    pub transaction_id: [u8; 3],
    pub options: Vec<Dhcp6Option>,
}

/// Parse a DHCPv6 message from `input`. Returns an error if the input is
/// shorter than 4 bytes or if any option's claimed length is inconsistent
/// with the remaining bytes.
pub fn parse(input: &[u8]) -> Result<Dhcp6Msg, String> {
    if input.len() < 4 {
        return Err(format!(
            "DHCPv6: message too short ({} bytes, need at least 4)",
            input.len()
        ));
    }
    let msg_type = input[0];
    let mut transaction_id = [0u8; 3];
    transaction_id.copy_from_slice(&input[1..4]);
    let mut options: Vec<Dhcp6Option> = Vec::new();
    let mut i = 4usize;
    while i < input.len() {
        if i + 4 > input.len() {
            return Err(format!(
                "DHCPv6: truncated option header at offset {}",
                i
            ));
        }
        let code = u16::from_be_bytes([input[i], input[i + 1]]);
        let len = u16::from_be_bytes([input[i + 2], input[i + 3]]) as usize;
        let value_start = i + 4;
        let value_end = value_start.checked_add(len).ok_or_else(|| {
            format!("DHCPv6: option length overflow at offset {}", i)
        })?;
        if value_end > input.len() {
            return Err(format!(
                "DHCPv6: option {} value truncated (need {} bytes, have {})",
                code,
                len,
                input.len() - value_start
            ));
        }
        options.push(Dhcp6Option {
            code,
            value: input[value_start..value_end].to_vec(),
        });
        i = value_end;
    }
    Ok(Dhcp6Msg {
        msg_type,
        transaction_id,
        options,
    })
}

/// Encode a DHCPv6 message back into bytes. Inverse of `parse`.
pub fn encode(msg: &Dhcp6Msg) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 8 * msg.options.len());
    out.push(msg.msg_type);
    out.extend_from_slice(&msg.transaction_id);
    for opt in &msg.options {
        let len = opt.value.len();
        // Lengths are 16-bit. Bail out silently if an option is oversized —
        // `parse` would have rejected such a value anyway.
        if len > u16::MAX as usize {
            return out;
        }
        out.extend_from_slice(&opt.code.to_be_bytes());
        out.extend_from_slice(&(len as u16).to_be_bytes());
        out.extend_from_slice(&opt.value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_solicit_no_options() {
        // SOLICIT (1) with xid 0x123456 and no options.
        let bytes = [0x01, 0x12, 0x34, 0x56];
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.msg_type, msg_type::SOLICIT);
        assert_eq!(msg.transaction_id, [0x12, 0x34, 0x56]);
        assert!(msg.options.is_empty());
    }

    #[test]
    fn parse_reply_with_clientid() {
        // REPLY (7) with xid 0x00FFFFFF carrying a CLIENTID option (code 1)
        // whose value is 4 bytes: 0xDE 0xAD 0xBE 0xEF.
        let bytes = [
            0x07, 0x00, 0xff, 0xff,
            0x00, 0x01, // code = CLIENTID
            0x00, 0x04, // length = 4
            0xde, 0xad, 0xbe, 0xef,
        ];
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.msg_type, msg_type::REPLY);
        assert_eq!(msg.transaction_id, [0x00, 0xff, 0xff]);
        assert_eq!(msg.options.len(), 1);
        assert_eq!(msg.options[0].code, option_code::CLIENTID);
        assert_eq!(msg.options[0].value, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn parse_multiple_options() {
        // REQUEST with two options: CLIENTID (code 1, 2 bytes) and
        // DNS_SERVERS (code 23, 32 bytes — two IPv6 addresses).
        let mut bytes = vec![0x03, 0xaa, 0xbb, 0xcc];
        bytes.extend_from_slice(&[
            0x00, 0x01, // CLIENTID
            0x00, 0x02, // length 2
            0x00, 0x01, // value
        ]);
        bytes.extend_from_slice(&[
            0x00, 0x17, // DNS_SERVERS (23)
            0x00, 0x20, // length 32
            0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
        ]);
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.msg_type, msg_type::REQUEST);
        assert_eq!(msg.options.len(), 2);
        assert_eq!(msg.options[0].code, option_code::CLIENTID);
        assert_eq!(msg.options[1].code, option_code::DNS_SERVERS);
        assert_eq!(msg.options[1].value.len(), 32);
        assert_eq!(
            msg.options[1].value[..8],
            [0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn parse_truncated_message() {
        // Less than 4 bytes → error.
        let bytes = [0x01, 0x02, 0x03];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_truncated_option_header() {
        // 4 bytes of header + a single trailing byte that can't fit a 4-byte
        // option header.
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x00];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_truncated_option_value() {
        // 4-byte header followed by a CLIENTID option header (code=1, len=10)
        // but only 4 bytes of value follow.
        let bytes = [0x07, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x0a, 1, 2, 3, 4];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn encode_solicit_empty_options() {
        let msg = Dhcp6Msg {
            msg_type: msg_type::SOLICIT,
            transaction_id: [0x11, 0x22, 0x33],
            options: Vec::new(),
        };
        assert_eq!(encode(&msg), vec![0x01, 0x11, 0x22, 0x33]);
    }

    #[test]
    fn round_trip_reply_with_options() {
        // REPLY + CLIENTID + IAADDR (code 5) with 24-byte value.
        let iaaddr = vec![0u8; 24];
        let msg = Dhcp6Msg {
            msg_type: msg_type::REPLY,
            transaction_id: [0xab, 0xcd, 0xef],
            options: vec![
                Dhcp6Option {
                    code: option_code::CLIENTID,
                    value: vec![0xaa, 0xbb],
                },
                Dhcp6Option {
                    code: option_code::IAADDR,
                    value: iaaddr.clone(),
                },
            ],
        };
        let bytes = encode(&msg);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.msg_type, msg_type::REPLY);
        assert_eq!(parsed.transaction_id, [0xab, 0xcd, 0xef]);
        assert_eq!(parsed.options.len(), 2);
        assert_eq!(parsed.options[0].code, option_code::CLIENTID);
        assert_eq!(parsed.options[0].value, vec![0xaa, 0xbb]);
        assert_eq!(parsed.options[1].code, option_code::IAADDR);
        assert_eq!(parsed.options[1].value, iaaddr);
    }

    #[test]
    fn round_trip_all_common_msg_types() {
        // Verify each well-known msg type round-trips with the same xid.
        let xid = [0xde, 0xad, 0xbe];
        for &mt in &[
            msg_type::SOLICIT,
            msg_type::ADVERTISE,
            msg_type::REQUEST,
            msg_type::CONFIRM,
            msg_type::RENEW,
            msg_type::REBIND,
            msg_type::REPLY,
            msg_type::RELEASE,
            msg_type::DECLINE,
            msg_type::RECONFIGURE,
            msg_type::INFORMATION_REQUEST,
            msg_type::RELAY_FORWARD,
            msg_type::RELAY_REPLY,
        ] {
            let msg = Dhcp6Msg {
                msg_type: mt,
                transaction_id: xid,
                options: Vec::new(),
            };
            let bytes = encode(&msg);
            let parsed = parse(&bytes).unwrap();
            assert_eq!(parsed.msg_type, mt, "msg_type round-trip mismatch");
            assert_eq!(parsed.transaction_id, xid);
            assert!(parsed.options.is_empty());
        }
    }

    #[test]
    fn round_trip_domain_list_option() {
        // DOMAIN_LIST option containing the DNS search list "example.com"
        // encoded as a single domain (length-prefixed): 07 'e' 'x' 'a' 'm'
        // 'p' 'l' 'e' 03 'c' 'o' 'm' 00
        let domain = vec![0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
                          0x03, b'c', b'o', b'm', 0x00];
        let msg = Dhcp6Msg {
            msg_type: msg_type::ADVERTISE,
            transaction_id: [0x01, 0x02, 0x03],
            options: vec![Dhcp6Option {
                code: option_code::DOMAIN_LIST,
                value: domain.clone(),
            }],
        };
        let bytes = encode(&msg);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.options.len(), 1);
        assert_eq!(parsed.options[0].code, option_code::DOMAIN_LIST);
        assert_eq!(parsed.options[0].value, domain);
    }

    #[test]
    fn parse_empty_after_header() {
        // 4 bytes of header, zero options is valid.
        let bytes = [0x0b, 0xff, 0xee, 0xdd];
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.msg_type, msg_type::INFORMATION_REQUEST);
        assert_eq!(msg.transaction_id, [0xff, 0xee, 0xdd]);
        assert!(msg.options.is_empty());
    }

    #[test]
    fn parse_zero_length_option_is_legal() {
        // PREFERENCE (code 7) with zero-length value.
        let bytes = [
            0x07, 0x10, 0x20, 0x30,
            0x00, 0x07, // code = PREFERENCE
            0x00, 0x00, // length 0
        ];
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.options.len(), 1);
        assert_eq!(msg.options[0].code, option_code::PREFERENCE);
        assert!(msg.options[0].value.is_empty());
    }
}