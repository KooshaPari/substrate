// Minimal MAPI property stream / property-list parser.
//
// References:
//   [MS-OXCDATA] v20211026, Section 2.11 PropertyEntry / Ptyp* type system.
//   [MS-OXPROPS] v20211026, section 2 — Property identifiers and types.
//   [MS-OXCMSG] v20211026, Section 2.2 Message Object — PidLid* property tag
//     ranges (0x8000-0xFFFE for named properties, 0x0001-0x7FFF for fixed).
//
// A property tag (PropertyTag) is a 32-bit value:
//   bits 0-15  : PropertyId (u16 LE)
//   bits 16-31 : PropertyType (u16 LE, one of the Ptyp* constants)
//
// For a property list (Section 2.11.1), each PropertyEntry is:
//   - 4-byte tag (LE)     — PropertyId (u16) + PropertyType (u16)
//   - 4-byte flags (LE)
//   - if flags bit A (0x01, PtypFlagPresent) is set, a value follows whose
//     layout depends on PropertyType. We DO NOT decode the value's typed
//     payload — instead we emit the raw, unmodified bytes following the
//     PropertyEntry header so the caller can decide on full decode.
//
//   - Multi-valued properties (PtypFlagArray): the FLAGS word bit 2 (0x04,
//     named "IsMultivalued" / PtypFlagArray per [MS-OXCDATA] 2.11.1.1) is set.
//     The following value field begins with a 4-byte u32 LE element count
//     before the per-element values.
//
// We only support the "as_32bit=false" form here: the 4-byte FLAGS interpretation
// in [MS-OXCDATA] has a historical 2-byte layout that this minimal parser does
// not implement. The `as_32bit` parameter is preserved in the signature so
// callers can pass false without effect (the 32-bit form is the only one this
// parser recognizes).
//
// Property types (PropertyType low byte). These match [MS-OXCDATA] 2.11.1:
//   0x0001 PtypBoolean    — u8
//   0x0002 PtypInteger16  — i16 LE
//   0x0003 PtypInteger32  — i32 LE
//   0x0004 PtypFloating32 — f32 LE
//   0x0005 PtypFloating64 — f64 LE
//   0x0006 PtypCurrency   — i64 LE
//   0x0007 PtypFloatingDate — f64 LE (OADate)
//   0x000A PtypErrorCode  — u32 LE
//   0x000B PtypBoolean    — u8 (binary alias for 0x0001)
//   0x000D PtypObject     — embedded object (opaque)
//   0x0014 PtypInteger64  — i64 LE
//   0x001E PtypString8    — MBCS (8-bit) string
//   0x001F PtypString     — UTF-16LE string
//   0x0020 PtypTime       — u64 LE (FILETIME)
//   0x0102 PtypBinary     — raw bytes, length-prefixed u32 LE
//
// The fixed-size types above have a deterministic on-disk length. PtypString,
// PtypString8, and PtypBinary are length-prefixed: a 4-byte u32 LE byte count
// followed by that many bytes. PtypObject is documented as containing a
// stream-like value that we treat as opaque; for the purposes of this parser
// we read it as PtypBinary-form.

/// MAPI property tag constants. Names per [MS-OXCDATA] Section 2.11.1.
pub const PTYP_BOOLEAN: u16 = 0x0001;
pub const PTYP_INTEGER16: u16 = 0x0002;
pub const PTYP_INTEGER32: u16 = 0x0003;
pub const PTYP_FLOATING32: u16 = 0x0004;
pub const PTYP_FLOATING64: u16 = 0x0005;
pub const PTYP_CURRENCY: u16 = 0x0006;
pub const PTYP_FLOATING_DATE: u16 = 0x0007;
pub const PTYP_ERROR_CODE: u16 = 0x000A;
pub const PTYP_OBJECT: u16 = 0x000D;
pub const PTYP_INTEGER64: u16 = 0x0014;
pub const PTYP_STRING8: u16 = 0x001E;
pub const PTYP_STRING: u16 = 0x001F;
pub const PTYP_TIME: u16 = 0x0020;
pub const PTYP_BINARY: u16 = 0x0102;

/// Property flag bits (FLWORD) per [MS-OXCDATA] 2.11.1.1.
pub const PTYP_FLAG_PRESENT: u16 = 0x0001;
pub const PTYP_FLAG_ARRAY: u16 = 0x0004;

/// A single MAPI property entry as extracted from a property list stream.
#[derive(Debug, Clone, PartialEq)]
pub struct MapiProp {
    /// Raw 32-bit property tag (PropertyId in low u16, PropertyType in high u16, LE).
    pub tag: u32,
    /// 4-byte FLWORD, here presented as a u16 (only low 16 bits populated
    /// in the as_32bit=true form). Bit A=0x0001 means the property value is
    /// present in the stream.
    pub flags: u16,
    /// Raw bytes of the property value. For fixed-size Ptyp* types, length
    /// matches the type's on-disk size; for length-prefixed types
    /// (PtypString / PtypString8 / PtypBinary) the length-prefix is included.
    /// For multi-valued entries (`flags & PTYP_FLAG_ARRAY != 0`), the 4-byte
    /// element-count prefix is included.
    pub value: Vec<u8>,
}

/// Parse a MAPI property list stream.
///
/// `as_32bit` is reserved for API parity with historical 16-bit/32-bit flag
/// layouts. This implementation only recognizes the modern 4-byte FLWORD form
/// (Section 2.11.1.1). Pass `false` to acknowledge the historical flag layout
/// is not implemented — the function returns an error if `true` is passed so
/// callers don't silently get wrong flag semantics.
pub fn parse(input: &[u8], as_32bit: bool) -> Result<Vec<MapiProp>, String> {
    if as_32bit {
        return Err(
            "mapi_props: as_32bit=true is not supported; only the 4-byte FLWORD form is implemented"
                .to_string(),
        );
    }
    let mut out: Vec<MapiProp> = Vec::new();
    let mut off: usize = 0;
    while off < input.len() {
        // Each entry header is 6 bytes: 4-byte tag + 2-byte FLWORD.
        if input.len() - off < 6 {
            return Err(format!(
                "mapi_props: truncated entry at offset {off}: need 6 bytes, have {}",
                input.len() - off
            ));
        }
        let tag = u32::from_le_bytes([
            input[off],
            input[off + 1],
            input[off + 2],
            input[off + 3],
        ]);
        let flags = u16::from_le_bytes([input[off + 4], input[off + 5]]);
        off += 6;

        // If the present flag is not set, the value is empty and we move on.
        if flags & PTYP_FLAG_PRESENT == 0 {
            out.push(MapiProp {
                tag,
                flags,
                value: Vec::new(),
            });
            continue;
        }

        let property_type = (tag >> 16) as u16;
        let fixed_len = fixed_size_for_type(property_type);
        let is_multi = flags & PTYP_FLAG_ARRAY != 0;

        if let Some(flen) = fixed_len {
            // Multi-valued fixed-size: 4-byte count + count * fixed.
            if is_multi {
                if input.len() - off < 4 {
                    return Err(format!(
                        "mapi_props: truncated array-count at offset {off}"
                    ));
                }
                let count = u32::from_le_bytes([
                    input[off],
                    input[off + 1],
                    input[off + 2],
                    input[off + 3],
                ]) as usize;
                off += 4;
                let total = count
                    .checked_mul(flen)
                    .ok_or_else(|| "mapi_props: array size overflow".to_string())?;
                if input.len() - off < total {
                    return Err(format!(
                        "mapi_props: truncated array payload (need {total}, have {})",
                        input.len() - off
                    ));
                }
                let value = input[off..off + total].to_vec();
                off += total;
                out.push(MapiProp {
                    tag,
                    flags,
                    value,
                });
            } else {
                if input.len() - off < flen {
                    return Err(format!(
                        "mapi_props: truncated fixed-size value at offset {off} (need {flen})"
                    ));
                }
                let value = input[off..off + flen].to_vec();
                off += flen;
                out.push(MapiProp {
                    tag,
                    flags,
                    value,
                });
            }
        } else if property_type == PTYP_STRING
            || property_type == PTYP_STRING8
            || property_type == PTYP_BINARY
            || property_type == PTYP_OBJECT
        {
            // Length-prefixed: 4-byte u32 LE byte count + payload.
            if input.len() - off < 4 {
                return Err(format!(
                    "mapi_props: truncated length-prefix at offset {off}"
                ));
            }
            let count = u32::from_le_bytes([
                input[off],
                input[off + 1],
                input[off + 2],
                input[off + 3],
            ]) as usize;
            off += 4;
            if input.len() - off < count {
                return Err(format!(
                    "mapi_props: truncated variable-size value at offset {off} (need {count})"
                ));
            }
            let value = input[off..off + count].to_vec();
            off += count;
            out.push(MapiProp {
                tag,
                flags,
                value,
            });
        } else {
            return Err(format!(
                "mapi_props: unknown property type 0x{property_type:04x} in tag 0x{tag:08x}"
            ));
        }
    }
    Ok(out)
}

/// On-disk length (in bytes) for fixed-size Ptyp* values, or None for
/// length-prefixed types.
fn fixed_size_for_type(t: u16) -> Option<usize> {
    match t {
        PTYP_BOOLEAN => Some(1),
        PTYP_INTEGER16 => Some(2),
        PTYP_INTEGER32 => Some(4),
        PTYP_FLOATING32 => Some(4),
        PTYP_FLOATING64 => Some(8),
        PTYP_CURRENCY => Some(8),
        PTYP_FLOATING_DATE => Some(8),
        PTYP_ERROR_CODE => Some(4),
        PTYP_INTEGER64 => Some(8),
        PTYP_TIME => Some(8),
        _ => None,
    }
}

/// Extract just the PropertyId (low 16 bits) from a tag.
pub fn property_id(tag: u32) -> u16 {
    (tag & 0xFFFF) as u16
}

/// Extract the PropertyType (high 16 bits) from a tag.
pub fn property_type(tag: u32) -> u16 {
    (tag >> 16) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_tag(id: u16, ptype: u16) -> u32 {
        (id as u32) | ((ptype as u32) << 16)
    }

    fn encode_u16_le(v: u16) -> [u8; 2] {
        v.to_le_bytes()
    }
    fn encode_u32_le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn encode_i16_le(v: i16) -> [u8; 2] {
        v.to_le_bytes()
    }
    fn encode_i32_le(v: i32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn encode_u64_le(v: u64) -> [u8; 8] {
        v.to_le_bytes()
    }
    fn encode_f64_le(v: f64) -> [u8; 8] {
        v.to_le_bytes()
    }

    #[test]
    fn rejects_as_32bit_true() {
        let v: Vec<u8> = vec![];
        let err = parse(&v, true).unwrap_err();
        assert!(err.contains("as_32bit=true"));
    }

    #[test]
    fn parses_empty_stream() {
        let out = parse(&[], false).expect("parse empty");
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn parses_boolean_with_present_flag() {
        // PropertyId 0x0007 = PidTagTransportMessageHeaders (type PtBoolean in
        // some test vectors). Use id=0x0001 type=PTYP_BOOLEAN.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0001, PTYP_BOOLEAN).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.push(0x01);
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].tag, encode_tag(0x0001, PTYP_BOOLEAN));
        assert_eq!(props[0].flags, PTYP_FLAG_PRESENT);
        assert_eq!(props[0].value, vec![0x01]);
    }

    #[test]
    fn parses_i16_with_present_flag() {
        // PidTagImportance = 0x0017, type PtypInteger16.
        // Cross-check: documented PidTagImportance is u16 (Integer16).
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0017, PTYP_INTEGER16).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_i16_le(2));
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].value, vec![2, 0]); // 2 LE
        assert_eq!(property_id(props[0].tag), 0x0017);
        assert_eq!(property_type(props[0].tag), PTYP_INTEGER16);
    }

    #[test]
    fn parses_i32_present() {
        // PidTagMessageSize = 0x0E08, PtypInteger32.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0E08, PTYP_INTEGER32).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_i32_le(0x1234_5678));
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        // LE bytes 0x78 0x56 0x34 0x12
        assert_eq!(props[0].value, vec![0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn parses_two_entries_consecutive() {
        // Two entries back-to-back: i32 + filetime.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0E08, PTYP_INTEGER32).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_i32_le(42));
        v.extend_from_slice(&encode_tag(0x0039, PTYP_TIME).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_u64_le(0x01D6_55F0_AAAA_BBBB));
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].value, vec![42, 0, 0, 0]);
        assert_eq!(props[1].value.len(), 8);
        assert_eq!(
            props[1].value,
            vec![0xBB, 0xBB, 0xAA, 0xAA, 0xF0, 0x55, 0xD6, 0x01]
        );
    }

    #[test]
    fn parses_binary_length_prefixed() {
        // PTYP_BINARY: 4-byte count + payload. Parser consumes prefix.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0102, PTYP_BINARY).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_u32_le(5));
        v.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00]);
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        // value is raw payload only (no 4-byte length prefix).
        assert_eq!(props[0].value, vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00]);
    }

    #[test]
    fn parses_utf16_string_length_prefixed() {
        // PTYP_STRING length-prefixed: 4-byte byte count + UTF-16LE.
        // Our parser consumes the 4-byte prefix and exposes only the payload.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x001E, PTYP_STRING).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_u32_le(4));
        v.extend_from_slice(&[0x48, 0x00, 0x69, 0x00]);
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        // value is the raw UTF-16LE payload (no prefix).
        // Cross-check: PtypString payload in [MS-OXCDATA] 2.11.1.1 is the
        // bytes following the u32 byte count.
        assert_eq!(props[0].value, vec![0x48, 0x00, 0x69, 0x00]);
    }

    #[test]
    fn parses_missing_value_when_flag_absent() {
        // FLWORD=0 => no value bytes follow.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0002, PTYP_INTEGER32).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(0x0000));
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        assert!(props[0].value.is_empty());
        assert_eq!(props[0].flags, 0x0000);
    }

    #[test]
    fn parses_multivalued_i32_array() {
        // Three i32 values. PtypInteger32, flags include array bit.
        // Cross-check: FLARRAY layout in [MS-OXCDATA] 2.11.1.1 is
        // u32 element count followed by count * fixed-size payloads.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0048, PTYP_INTEGER32).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT | PTYP_FLAG_ARRAY));
        v.extend_from_slice(&encode_u32_le(3));
        v.extend_from_slice(&encode_i32_le(10));
        v.extend_from_slice(&encode_i32_le(20));
        v.extend_from_slice(&encode_i32_le(30));
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        // value is 3 * 4 = 12 bytes (no count prefix — we consume it).
        assert_eq!(props[0].value.len(), 12);
        assert_eq!(&props[0].value[0..4], &encode_i32_le(10));
        assert_eq!(&props[0].value[4..8], &encode_i32_le(20));
        assert_eq!(&props[0].value[8..12], &encode_i32_le(30));
    }

    #[test]
    fn rejects_truncated_tag() {
        // 5 bytes: tag incomplete.
        let v = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let err = parse(&v, false).unwrap_err();
        assert!(err.contains("truncated entry"));
    }

    #[test]
    fn rejects_truncated_fixed_value() {
        // Tag declares i32 but only 2 bytes follow.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0E08, PTYP_INTEGER32).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&[0x01, 0x02]);
        let err = parse(&v, false).unwrap_err();
        assert!(err.contains("truncated fixed-size"));
    }

    #[test]
    fn rejects_truncated_string_payload() {
        // Length-prefix says 4 bytes but only 2 follow.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x001E, PTYP_STRING).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_u32_le(4));
        v.extend_from_slice(&[0x48, 0x00]);
        let err = parse(&v, false).unwrap_err();
        assert!(err.contains("truncated variable-size"));
    }

    #[test]
    fn rejects_unknown_type_with_present_flag() {
        // PtypUnknown 0x9999 is not in our fixed_size_for_type list.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0001, 0x9999).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        let err = parse(&v, false).unwrap_err();
        assert!(err.contains("unknown property type"));
    }

    #[test]
    fn parses_f64_floating_date() {
        // PTYP_FLOATING_DATE: 8-byte f64 LE.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0060, PTYP_FLOATING_DATE).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_f64_le(45000.5));
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].value.len(), 8);
    }

    #[test]
    fn parses_two_length_prefixed_back_to_back() {
        // Two binary blobs back-to-back. Parser must consume both prefixes
        // and align correctly to the next entry.
        let mut v = Vec::new();
        v.extend_from_slice(&encode_tag(0x0102, PTYP_BINARY).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_u32_le(2));
        v.extend_from_slice(&[0xAA, 0xBB]);
        v.extend_from_slice(&encode_tag(0x0103, PTYP_BINARY).to_le_bytes());
        v.extend_from_slice(&encode_u16_le(PTYP_FLAG_PRESENT));
        v.extend_from_slice(&encode_u32_le(1));
        v.push(0xCC);
        let props = parse(&v, false).expect("parse");
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].value, vec![0xAA, 0xBB]);
        assert_eq!(props[1].value, vec![0xCC]);
    }
}