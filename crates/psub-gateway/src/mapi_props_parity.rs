//! Minimal MAPI property-entry build + round-trip parity helper.
//!
//! This is the parity counterpart to [`crate::mapi_props`]:
//! it builds wire-format MAPI `PropertyEntry` records (per
//! [MS-OXCDATA] v20211026 §2.11) and asserts that the existing
//! `mapi_props::parse` round-trips every byte back to the input
//! fields.
//!
//! The existing `mapi_props` is parsing-only. This module is
//! intentionally narrow: it produces a single `PropertyEntry` at a
//! time and only covers the modern 6-byte header layout (4-byte tag +
//! 2-byte FLWORD). The historical 2-byte layout is documented as not
//! implemented in `mapi_props`.
//!
//! Wire layout for a single entry ([MS-OXCDATA] 2.11.1):
//!
//! ```text
//!   4 bytes : PropertyTag (LE)
//!             bits 0-15  = PropertyId (u16)
//!             bits 16-31 = PropertyType (u16)
//!   2 bytes : FLAGS (LE u16)
//!             bit 0 = PtypFlagPresent
//!             bit 2 = PtypFlagArray
//!   variable: VALUE
//!             - if PtypFlagPresent is unset: nothing
//!             - else if PtypFlagArray is set: u32 LE element count
//!               followed by count * fixed-size payloads
//!             - else if PropertyType is fixed-size: that many bytes
//!             - else if PropertyType is variable-size: u32 LE byte
//!               count followed by that many bytes
//! ```

use crate::mapi_props;

/// Build a single MAPI PropertyEntry on the wire.
///
/// `tag` is a packed `PropertyId | (PropertyType << 16)` value (LE in
/// memory on little-endian hosts). `flags` is the 2-byte FLWORD — pass
/// [`mapi_props::PTYP_FLAG_PRESENT`] when `value` should be emitted,
/// [`mapi_props::PTYP_FLAG_PRESENT | mapi_props::PTYP_FLAG_ARRAY`]
/// when `value` starts with a u32 element count followed by
/// `count * fixed_size` payloads.
///
/// The producer is responsible for emitting `value` in the correct
/// layout for `property_type(tag)` — this function does not validate
/// it because the parser is the source of truth for what counts as a
/// well-formed value.
pub fn build_entry(tag: u32, flags: u16, value: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(6 + value.len());
    buf.extend_from_slice(&tag.to_le_bytes());
    buf.extend_from_slice(&flags.to_le_bytes());
    if (flags & mapi_props::PTYP_FLAG_PRESENT) != 0 {
        buf.extend_from_slice(value);
    }
    buf
}

/// Build a single-entry buffer then re-parse it via
/// [`mapi_props::parse`] and assert every field round-trips. Panics
/// on the first mismatch with a descriptive message.
///
/// `expected_value` is the *raw value bytes after the 6-byte header*;
/// it must match what the parser exposes as `MapiProp.value`. For
/// length-prefixed variable types (PtypString / PtypString8 /
/// PtypBinary) and arrays, the parser has already consumed the
/// 4-byte prefix(es) so `expected_value` is the payload only.
pub fn assert_round_trip(tag: u32, flags: u16, value: &[u8], expected_value: &[u8]) {
    let buf = build_entry(tag, flags, value);
    let props = mapi_props::parse(&buf, false)
        .expect("mapi_props_parity: parser rejected a buffer we just built");
    if props.len() != 1 {
        panic!(
            "mapi_props_parity: expected exactly 1 property, got {}",
            props.len()
        );
    }
    let p = &props[0];
    if p.tag != tag {
        panic!(
            "mapi_props_parity: tag mismatch expected=0x{:08x} got=0x{:08x}",
            tag, p.tag
        );
    }
    if p.flags != flags {
        panic!(
            "mapi_props_parity: flags mismatch expected=0x{:04x} got=0x{:04x}",
            flags, p.flags
        );
    }
    if p.value != expected_value {
        panic!(
            "mapi_props_parity: value mismatch expected={:02x?} got={:02x?}",
            expected_value, p.value
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapi_props::{
        PTYP_BINARY, PTYP_BOOLEAN, PTYP_FLOATING32, PTYP_FLOATING64, PTYP_FLOATING_DATE,
        PTYP_INTEGER16, PTYP_INTEGER32, PTYP_INTEGER64, PTYP_STRING, PTYP_STRING8, PTYP_TIME,
        PTYP_FLAG_ARRAY, PTYP_FLAG_PRESENT,
    };

    // Helper: build a 32-bit PropertyTag from (PropertyId, PropertyType).
    fn tag(id: u16, ty: u16) -> u32 {
        ((ty as u32) << 16) | (id as u32)
    }

    #[test]
    fn build_boolean_present_round_trips() {
        // [MS-OXCDATA] 2.11.1: PtypBoolean is a fixed-size 1-byte value.
        assert_round_trip(tag(0x0001, PTYP_BOOLEAN), PTYP_FLAG_PRESENT, &[0x01], &[0x01]);
    }

    #[test]
    fn build_i16_present_round_trips() {
        // PidTagImportance = 0x0017, PtypInteger16. Cross-checked
        // against [MS-OXPROPS] 2.6.1 — PidTagImportance is documented
        // as PtypInteger16 (u16 LE).
        let v = 2i16.to_le_bytes();
        assert_round_trip(tag(0x0017, PTYP_INTEGER16), PTYP_FLAG_PRESENT, &v, &v);
    }

    #[test]
    fn build_i32_present_round_trips() {
        // PidTagMessageSize = 0x0E08, PtypInteger32.
        let v = 0x1234_5678i32.to_le_bytes();
        assert_round_trip(tag(0x0E08, PTYP_INTEGER32), PTYP_FLAG_PRESENT, &v, &v);
    }

    #[test]
    fn build_i64_present_round_trips() {
        // PtypInteger64 — 8 bytes LE.
        let v = 0x0102_0304_0506_0708i64.to_le_bytes();
        assert_round_trip(tag(0x0014, PTYP_INTEGER64), PTYP_FLAG_PRESENT, &v, &v);
    }

    #[test]
    fn build_f64_present_round_trips() {
        // PtypFloating64 — 8 bytes LE, IEEE-754.
        let v = 3.141592653589793_f64.to_le_bytes();
        assert_round_trip(tag(0x0005, PTYP_FLOATING64), PTYP_FLAG_PRESENT, &v, &v);
    }

    #[test]
    fn build_f32_present_round_trips() {
        // PtypFloating32 — 4 bytes LE.
        let v = 2.5_f32.to_le_bytes();
        assert_round_trip(tag(0x0004, PTYP_FLOATING32), PTYP_FLAG_PRESENT, &v, &v);
    }

    #[test]
    fn build_filetime_present_round_trips() {
        // PtypTime — 8 bytes LE FILETIME. Cross-checked by encoding
        // and comparing against the existing `mapi_props` test fixture
        // which uses 0x01D6_55F0_AAAA_BBBB.
        let v = 0x01D6_55F0_AAAA_BBBBu64.to_le_bytes();
        assert_round_trip(tag(0x0039, PTYP_TIME), PTYP_FLAG_PRESENT, &v, &v);
    }

    #[test]
    fn build_floating_date_present_round_trips() {
        // PtypFloatingDate — f64 LE OADate.
        let v = 45000.5_f64.to_le_bytes();
        assert_round_trip(
            tag(0x0060, PTYP_FLOATING_DATE),
            PTYP_FLAG_PRESENT,
            &v,
            &v,
        );
    }

    #[test]
    fn build_binary_length_prefixed_round_trips() {
        // [MS-OXCDATA] 2.11.1: PtypBinary is length-prefixed u32 LE +
        // payload. The parser consumes the 4-byte prefix; expected_value
        // is the payload only.
        let payload = [0xDE, 0xAD, 0xBE, 0xEF, 0x00];
        let mut on_wire = Vec::new();
        on_wire.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        on_wire.extend_from_slice(&payload);
        assert_round_trip(
            tag(0x0102, PTYP_BINARY),
            PTYP_FLAG_PRESENT,
            &on_wire,
            &payload,
        );
    }

    #[test]
    fn build_utf16_string_length_prefixed_round_trips() {
        // [MS-OXCDATA] 2.11.1: PtypString is length-prefixed u32 LE byte
        // count + UTF-16LE bytes. Parser strips the prefix.
        let payload = [0x48, 0x00, 0x69, 0x00]; // "Hi" in UTF-16LE
        let mut on_wire = Vec::new();
        on_wire.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        on_wire.extend_from_slice(&payload);
        assert_round_trip(
            tag(0x001E, PTYP_STRING),
            PTYP_FLAG_PRESENT,
            &on_wire,
            &payload,
        );
    }

    #[test]
    fn build_string8_length_prefixed_round_trips() {
        // [MS-OXCDATA] 2.11.1: PtypString8 is length-prefixed u32 LE +
        // MBCS payload.
        let payload = *b"hello";
        let mut on_wire = Vec::new();
        on_wire.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        on_wire.extend_from_slice(&payload);
        assert_round_trip(
            tag(0x001F, PTYP_STRING8),
            PTYP_FLAG_PRESENT,
            &on_wire,
            &payload,
        );
    }

    #[test]
    fn build_absent_flag_emits_no_value_round_trips() {
        // FLAGS=0 means "no value present" — the parser must report an
        // empty value field. The on-wire bytes after the 6-byte header
        // are intentionally absent; this matches [MS-OXCDATA] 2.11.1.
        assert_round_trip(tag(0x0002, PTYP_INTEGER32), 0x0000, &[], &[]);
    }

    #[test]
    fn build_multivalued_i32_array_round_trips() {
        // [MS-OXCDATA] 2.11.1.1: PtypFlagArray layout is u32 LE element
        // count followed by count * fixed-size payloads. Parser strips
        // the count; expected_value is the raw 12 bytes of payloads.
        let mut on_wire = Vec::new();
        on_wire.extend_from_slice(&3u32.to_le_bytes());
        on_wire.extend_from_slice(&10i32.to_le_bytes());
        on_wire.extend_from_slice(&20i32.to_le_bytes());
        on_wire.extend_from_slice(&30i32.to_le_bytes());
        let mut expected = Vec::new();
        expected.extend_from_slice(&10i32.to_le_bytes());
        expected.extend_from_slice(&20i32.to_le_bytes());
        expected.extend_from_slice(&30i32.to_le_bytes());
        assert_round_trip(
            tag(0x0048, PTYP_INTEGER32),
            PTYP_FLAG_PRESENT | PTYP_FLAG_ARRAY,
            &on_wire,
            &expected,
        );
    }

    #[test]
    fn build_matches_existing_parser_fixture_layout() {
        // Cross-check: the `mapi_props::tests::parses_i32_present`
        // fixture hand-builds a buffer using `encode_tag` (which we
        // reproduce via `tag()`) + `encode_u16_le(PTYP_FLAG_PRESENT)`
        // + 4 bytes LE. Our builder must produce the same 6 + 4 = 10
        // bytes.
        let buf = build_entry(
            tag(0x0E08, PTYP_INTEGER32),
            PTYP_FLAG_PRESENT,
            &0x1234_5678i32.to_le_bytes(),
        );
        assert_eq!(buf.len(), 6 + 4);
        assert_eq!(&buf[0..4], &tag(0x0E08, PTYP_INTEGER32).to_le_bytes());
        assert_eq!(&buf[4..6], &PTYP_FLAG_PRESENT.to_le_bytes());
        assert_eq!(&buf[6..10], &0x1234_5678i32.to_le_bytes());
    }
}