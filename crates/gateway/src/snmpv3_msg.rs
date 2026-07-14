// Minimal SNMPv3 message parser (RFC 3412). RFC 3412 §6.4 defines the
// SNMPv3 message as an SNMP message with the following top-level
// structure (encoded using the SNMP standard ASN.1 BER subset):
//
//   SEQUENCE {
//       INTEGER       msgVersion     -- must be 3 (SNMPv3)
//       SEQUENCE {                  -- msgGlobalData
//           INTEGER   msgID
//           INTEGER   msgMaxSize
//           OCTET STRING (SIZE (0..255)) msgFlags
//           INTEGER   msgSecurityModel
//       }
//       OCTET STRING                -- msgSecurityParameters (opaque to
//                                   -- the dispatcher; USM-decoded per
//                                   -- RFC 3414 §3)
//       ANY                          -- scopedPDU (SEQUENCE in practice)
//   }
//
// Layout reproduced verbatim from RFC 3412 §6.4:
//
//     SnmpV3Message ::= SEQUENCE {
//         msgVersion                 INTEGER (0 .. 2147483647),
//         msgGlobalData              MsgGlobalData,
//         msgSecurityParameters      OCTET STRING (SIZE(0..65535)),
//         scopedPDU                  ScopedPDU
//     }
//
//     MsgGlobalData ::= SEQUENCE {
//         msgID                      INTEGER (0..2147483647),
//         msgMaxSize                 INTEGER (484..2147483647),
//         msgFlags                   OCTET STRING (SIZE(1)),
//         msgSecurityModel           INTEGER (1..2147483647)
//     }
//
// `msgFlags` byte: bits 0..2 are (least significant bit first):
//     bit0 = reportableFlag, bit1 = privFlag, bit2 = authFlag
//
// This parser only decodes the top-level structure (and the inner
// `msgGlobalData` integers). It does NOT validate USM (RFC 3414)
// crypto, scopedPDU authorization, or dispatcher-level rules — those
// are callers' responsibility. The opaque `security_params` and
// `scoped_pdu` byte vectors are returned as-is for downstream handling.

/// SNMPv3 flag bits within the `msgFlags` octet. Per RFC 3412 §6.4 the
/// octet is 1 byte; per RFC 3414 §4 the layout is:
///     bit0 (LSB) = reportable
///     bit1 = privFlag (encrypted payload)
///     bit2 = authFlag (authenticated)
/// The remaining bits (3..7) are reserved and MUST be zero.
pub const FLAG_REPORTABLE: u8 = 1 << 0;
pub const FLAG_PRIV: u8 = 1 << 1;
pub const FLAG_AUTH: u8 = 1 << 2;

/// RFC 3414 §4 secLevel enum values. These are the integer values
/// that appear in USM-level structures (e.g. the `usmStatsSecLevel`
/// object), and they correspond to combinations of `authFlag` /
/// `privFlag` bits in `msgFlags`:
///     noAuthNoPriv(1) = priv=0, auth=0
///     authNoPriv(2)  = priv=0, auth=1
///     authPriv(3)    = priv=1, auth=1
pub const SECLEVEL_NOAUTH_NOPRIV: u8 = 1;
pub const SECLEVEL_AUTH_NOPRIV: u8 = 2;
pub const SECLEVEL_AUTH_PRIV: u8 = 3;

/// Decoded top-level SNMPv3 message.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct V3Msg {
    /// Raw `msgFlags` byte as carried in the wire format (1 byte).
    pub flags: u8,
    /// Convenience mapping of `flags & 0b0000_0111` to the three
    /// `reportable / priv / auth` semantics from RFC 3414 §4:
    ///     0 = noAuthNoPriv, 1 = authNoPriv, 3 = authPriv.
    pub security_level: u8,
    /// msgID from `msgGlobalData` (RFC 3412 §6.4).
    pub msg_id: u32,
    /// msgMaxSize from `msgGlobalData`.
    pub msg_max_size: u32,
    /// msgSecurityModel from `msgGlobalData` (e.g. 3 = USM).
    pub security_model: u32,
    /// Opaque bytes of the `msgSecurityParameters` OCTET STRING.
    pub security_params: Vec<u8>,
    /// Opaque bytes of the `scopedPDU` SEQUENCE.
    pub scoped_pdu: Vec<u8>,
    /// Entire top-level BER TLV for `msgVersion` (the `02 01 03`
    /// prefix). Preserved for callers that want to re-emit the
    /// version prefix verbatim.
    pub raw_header: Vec<u8>,
    /// Convenience alias for `security_params` (same bytes, named to
    /// match the task spec).
    pub data: Vec<u8>,
}

/// Parse the top-level structure of an SNMPv3 message per RFC 3412
/// §6.4. Returns `V3Msg` with the fixed-prefix fields decoded and the
/// security-parameters / scoped-PDU regions returned as opaque bytes.
/// USM (RFC 3414) crypto, scopedPDU authorization, and message
/// dispatcher rules are out of scope — see module docstring.
pub fn parse(input: &[u8]) -> Result<V3Msg, String> {
    // msgVersion: INTEGER 3 → tag=0x02, length=0x01, value=0x03.
    if input.len() < 3 {
        return Err(format!(
            "SNMPv3 too short: need at least 3 bytes for msgVersion, got {}",
            input.len()
        ));
    }
    if input[0] != 0x02 || input[1] != 0x01 {
        return Err(format!(
            "expected INTEGER tag for msgVersion, got 0x{:02x} len 0x{:02x}",
            input[0], input[1]
        ));
    }
    if input[2] != 0x03 {
        return Err(format!("expected msgVersion=3 (SNMPv3), got {}", input[2]));
    }
    let raw_header = input[..3].to_vec();
    let mut cursor = 3usize;

    // msgGlobalData: SEQUENCE { ... }
    let (mg_consumed, msg_id, msg_max_size, flags, security_model) =
        parse_msg_global_data(&input[cursor..])?;
    cursor += mg_consumed;

    // msgSecurityParameters: OCTET STRING
    let (sp_consumed, security_params) =
        parse_octet_string(&input[cursor..], "msgSecurityParameters")?;
    cursor += sp_consumed;

    // scopedPDU: anything from cursor to end of input. RFC 3412
    // describes it as `ANY` defined to be a `ScopedPDUData` for
    // dispatcher processing; we treat it as opaque bytes.
    let scoped_pdu = input[cursor..].to_vec();
    let data = security_params.clone();
    // RFC 3414 §4 secLevel mapping from msgFlags bits 1..2.
    let security_level = match (flags & FLAG_AUTH, flags & FLAG_PRIV) {
        (0, 0) => SECLEVEL_NOAUTH_NOPRIV,
        (_, 0) => SECLEVEL_AUTH_NOPRIV,
        (_, _) => SECLEVEL_AUTH_PRIV,
    };
    Ok(V3Msg {
        flags,
        security_level,
        msg_id,
        msg_max_size,
        security_model,
        security_params,
        scoped_pdu,
        raw_header,
        data,
    })
}

fn parse_msg_global_data(input: &[u8]) -> Result<(usize, u32, u32, u8, u32), String> {
    if input.len() < 2 {
        return Err(format!(
            "msgGlobalData too short: need at least 2 bytes, got {}",
            input.len()
        ));
    }
    if input[0] != 0x30 {
        return Err(format!(
            "expected SEQUENCE tag (0x30) for msgGlobalData, got 0x{:02x}",
            input[0]
        ));
    }
    let (content_consumed, content_len) =
        read_ber_length(&input[1..]).map_err(|e| format!("msgGlobalData length: {e}"))?;
    let total_consumed = 1 + content_consumed + content_len;
    if input.len() < total_consumed {
        return Err(format!(
            "msgGlobalData truncated: need {} bytes, got {}",
            total_consumed,
            input.len()
        ));
    }
    let mut cursor = 1 + content_consumed;
    let end = cursor + content_len;

    // INTEGER msgID
    let (c, msg_id) = parse_integer(&input[cursor..], "msgID")?;
    cursor += c;
    // INTEGER msgMaxSize
    let (c, msg_max_size) = parse_integer(&input[cursor..], "msgMaxSize")?;
    cursor += c;
    // OCTET STRING msgFlags (must be SIZE(1) per RFC 3412 §6.4)
    let (c, flags) = parse_octet_string_one(&input[cursor..], "msgFlags")?;
    cursor += c;
    // INTEGER msgSecurityModel
    let (c, security_model) = parse_integer(&input[cursor..], "msgSecurityModel")?;
    cursor += c;
    if cursor != end {
        return Err(format!(
            "msgGlobalData trailing bytes: parsed {} of {}",
            cursor - (1 + content_consumed),
            content_len
        ));
    }
    Ok((total_consumed, msg_id, msg_max_size, flags, security_model))
}

fn parse_integer(input: &[u8], field: &str) -> Result<(usize, u32), String> {
    if input.len() < 2 || input[0] != 0x02 {
        return Err(format!(
            "expected INTEGER tag for {field}, got 0x{:02x}",
            input[0]
        ));
    }
    let (lc, len) = read_ber_length(&input[1..]).map_err(|e| format!("{field} length: {e}"))?;
    let total = 1 + lc + len;
    if input.len() < total {
        return Err(format!("{field} truncated"));
    }
    let mut value: u32 = 0;
    let value_start = 1 + lc;
    let value_end = value_start + len;
    for byte in &input[value_start..value_end] {
        value = (value << 8) | u32::from(*byte);
    }
    Ok((total, value))
}

fn parse_octet_string(input: &[u8], field: &str) -> Result<(usize, Vec<u8>), String> {
    if input.is_empty() || input[0] != 0x04 {
        return Err(format!(
            "expected OCTET STRING tag for {field}, got 0x{:02x}",
            input[0]
        ));
    }
    let (lc, len) = read_ber_length(&input[1..]).map_err(|e| format!("{field} length: {e}"))?;
    let total = 1 + lc + len;
    if input.len() < total {
        return Err(format!("{field} truncated"));
    }
    let value_start = 1 + lc;
    let value_end = value_start + len;
    Ok((total, input[value_start..value_end].to_vec()))
}

fn parse_octet_string_one(input: &[u8], field: &str) -> Result<(usize, u8), String> {
    if input.len() < 3 || input[0] != 0x04 || input[1] != 0x01 {
        return Err(format!(
            "expected OCTET STRING SIZE(1) for {field}, got 0x{:02x} len 0x{:02x}",
            input[0], input[1]
        ));
    }
    Ok((3, input[2]))
}

fn read_ber_length(input: &[u8]) -> Result<(usize, usize), String> {
    if input.is_empty() {
        return Err("BER length missing".to_string());
    }
    let first = input[0];
    if first < 0x80 {
        Ok((1, usize::from(first)))
    } else {
        let n = usize::from(first & 0x7F);
        if n == 0 || n > 4 {
            return Err(format!("BER length n={n} unsupported"));
        }
        if input.len() < 1 + n {
            return Err("BER length truncated".to_string());
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | usize::from(input[1 + i]);
        }
        Ok((1 + n, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference fixture generated by an external Python script that
    /// encodes an SNMPv3 message per RFC 3412 §6.4:
    ///
    ///     msgVersion        = INTEGER 3
    ///     msgGlobalData.SEQUENCE {
    ///         msgID          = 1
    ///         msgMaxSize     = 1500
    ///         msgFlags       = 0x00 (noAuthNoPriv, no reportable)
    ///         msgSecurityModel = 3 (USM)
    ///     }
    ///     msgSecurityParameters = OCTET STRING (12 zero bytes)
    ///     scopedPDU        = SEQUENCE { 80 02 01 03 AA BB CC }
    ///
    /// Hex bytes (41 total):
    ///   020103 300d 020101 020205dc 040100 020103
    ///   040c 000000000000000000000000
    ///   3007 80020103 aabbcc
    fn rfc3412_fixture() -> Vec<u8> {
        vec![
            0x02, 0x01, 0x03, // msgVersion INTEGER 3
            0x30, 0x0d, // SEQUENCE len 13
            0x02, 0x01, 0x01, // msgID INTEGER 1
            0x02, 0x02, 0x05, 0xdc, // msgMaxSize INTEGER 1500
            0x04, 0x01, 0x00, // msgFlags OCTET STRING (1 byte) 0x00
            0x02, 0x01, 0x03, // msgSecurityModel INTEGER 3
            0x04, 0x0c, // OCTET STRING len 12
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30,
            0x07, // SEQUENCE len 7 (scopedPDU)
            0x80, 0x02, 0x01, 0x03, 0xaa, 0xbb, 0xcc,
        ]
    }

    /// Round-trip the RFC-3412 fixture: parse it and verify every
    /// fixed-prefix field matches the encoding.
    #[test]
    fn parse_rfc3412_fixture() {
        let bytes = rfc3412_fixture();
        let msg = parse(&bytes).expect("parse should succeed");
        assert_eq!(msg.flags, 0x00);
        assert_eq!(msg.security_level, 1);
        assert_eq!(msg.msg_id, 1);
        assert_eq!(msg.msg_max_size, 1500);
        assert_eq!(msg.security_model, 3);
        assert_eq!(msg.raw_header, vec![0x02, 0x01, 0x03]);
        assert_eq!(msg.security_params.len(), 12);
        assert!(msg.security_params.iter().all(|&b| b == 0));
        // scopedPDU bytes: 30 07 80 02 01 03 aa bb cc
        assert_eq!(
            msg.scoped_pdu,
            vec![0x30, 0x07, 0x80, 0x02, 0x01, 0x03, 0xaa, 0xbb, 0xcc]
        );
    }

    /// RFC 3414 §4 defines the security-level mapping:
    ///     noAuthNoPriv = 1, authNoPriv = 2, authPriv = 3.
    /// The `security_level` field of `V3Msg` is the RFC 3414 enum
    /// value derived from the auth/priv bits in `msgFlags`.
    #[test]
    fn parse_security_levels() {
        let mut bytes = rfc3412_fixture();
        // msgFlags is at offset 14 in the fixture (after version+SEQUENCE+2 ints).
        // bytes[0..=2]  = 02 01 03        (msgVersion)
        // bytes[3..=4]  = 30 0d           (SEQUENCE tag+len)
        // bytes[5..=7]  = 02 01 01        (msgID)
        // bytes[8..=11] = 02 02 05 dc     (msgMaxSize)
        // bytes[12..=14]= 04 01 xx        (msgFlags)
        // byte 14 is the flags byte.
        bytes[14] = 0x00; // noAuthNoPriv
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.flags, 0x00);
        assert_eq!(
            msg.security_level, 1,
            "noAuthNoPriv maps to 1 per RFC 3414 §4"
        );

        bytes[14] = FLAG_AUTH; // 0b00000100 = auth only → authNoPriv
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.flags, FLAG_AUTH);
        assert_eq!(
            msg.security_level, 2,
            "authNoPriv maps to 2 per RFC 3414 §4"
        );

        bytes[14] = FLAG_AUTH | FLAG_PRIV; // 0b00000110 → authPriv
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.security_level, 3, "authPriv maps to 3 per RFC 3414 §4");
    }

    /// Reject messages that start with anything other than the
    /// `02 01 03` version prefix.
    #[test]
    fn parse_rejects_wrong_version() {
        let bytes = vec![0x02, 0x01, 0x01]; // SNMPv1
        assert!(parse(&bytes).is_err());
    }

    /// Reject empty or truncated input.
    #[test]
    fn parse_rejects_short_input() {
        assert!(parse(&[]).is_err());
        assert!(parse(&[0x02]).is_err());
        assert!(parse(&[0x02, 0x01]).is_err());
    }

    /// The raw_header field must be exactly the three bytes
    /// `02 01 03` per the RFC §6.4 encoding rule.
    #[test]
    fn raw_header_is_three_bytes() {
        let bytes = rfc3412_fixture();
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.raw_header.len(), 3);
        assert_eq!(msg.raw_header, vec![0x02, 0x01, 0x03]);
    }

    /// `data` is a convenience alias for `security_params`. Verify
    /// both reference the same content.
    #[test]
    fn data_aliases_security_params() {
        let bytes = rfc3412_fixture();
        let msg = parse(&bytes).unwrap();
        assert_eq!(msg.data, msg.security_params);
    }

    /// Truncated msgGlobalData (SEQUENCE length exceeds buffer) must
    /// be rejected.
    #[test]
    fn parse_truncated_msg_global_data() {
        // Version is fine, but msgGlobalData declares 13 bytes and
        // we only provide 2.
        let bytes = vec![0x02, 0x01, 0x03, 0x30, 0x0d, 0x02, 0x01];
        assert!(parse(&bytes).is_err());
    }
}
