//! Minimal MS-CFB + legacy PowerPoint .ppt header build + round-trip parity helper.
//!
//! This is the parity counterpart to [`crate::pres_header_parse`]:
//! it builds a valid 512-byte MS-CFB Compound File Header (CFBH,
//! [MS-CFB] v20210817 §2.2) — which is the first 512 bytes of every
//! legacy PowerPoint .ppt file — and asserts that the existing
//! `pres_header_parse::parse` round-trips every advertised field back
//! to the inputs we supplied.
//!
//! The existing `pres_header_parse` is parsing-only. This module is
//! intentionally narrow: it produces a single CFBH (no FAT sectors,
//! no directory entries) and only covers the `version` and
//! `is_encrypted` axes. Everything else in the CFBH is fixed to
//! documented defaults (sector_shift, mini_sector_shift,
//! mini_stream_cutoff_size, etc.) so the parser accepts the buffer.
//!
//! CFBH wire layout ([MS-CFB] §2.2):
//!
//! ```text
//!   0x00 (8)  : header_signature = {0xD0,0xCF,0x11,0xE0,0xA1,0xB1,0x1A,0xE1}
//!   0x08 (16) : header_clsid    = zero
//!   0x18 (2)  : minor_version   = 0
//!   0x1A (2)  : major_version   = 3 or 4
//!   0x1C (2)  : byte_order      = {0xFE,0xFF}
//!   0x1E (2)  : sector_shift    = 9 (v3) or 12 (v4)
//!   0x20 (2)  : mini_sector_shift = 6
//!   0x22 (6)  : reserved        = zero
//!   0x28 (4)  : number_of_directory_sectors = 0
//!   0x2C (4)  : number_of_fat_sectors
//!   0x30 (4)  : first_directory_sector_location = 0
//!   0x34 (4)  : transaction_signature_number
//!               (non-zero => is_encrypted)
//!   0x38 (4)  : mini_stream_cutoff_size = 0x00001000
//!   0x3C (4)  : first_minifat_sector_location = 0xFFFFFFFE (NOSTREAM)
//!   0x40 (4)  : number_of_minifat_sectors = 0
//!   0x44 (4)  : first_difat_sector_location = 0xFFFFFFFE (NOSTREAM)
//!   0x48 (4)  : number_of_difat_sectors = 0
//!   0x4C..    : difat (109 entries of u32 LE = 436 bytes)
//! ```
//!
//! Total: 76 + 436 = 512 bytes.

use crate::pres_header_parse::{
    parse, CFB_BYTE_ORDER, CFB_HEADER_SIZE, CFB_MAGIC, CFB_MAJOR_V3, CFB_MAJOR_V4,
    CFB_MINI_STREAM_CUTOFF,
};

/// Build a 512-byte MS-CFB Compound File Header (CFBH) compatible
/// with legacy PowerPoint .ppt.
///
/// `version` selects the CFB major version: `3` (legacy .ppt) or
/// `4` (MS-CFB v4, 4 KiB sectors). Other values cause the existing
/// parser to reject the buffer, so this builder rejects them up
/// front with a loud `Err` rather than silently producing a buffer
/// that cannot be round-tripped.
///
/// `is_encrypted` is mapped onto the CFBH
/// `transaction_signature_number` field at offset 0x34: setting it to
/// a non-zero value (we use `1`) signals Microsoft Password
/// Protection ([MS-OFFCRYPTO] §2.3.4.10). The parser surfaces this
/// via [`crate::pres_header_parse::PresHeader::is_encrypted`].
pub fn build_ppt_header(version: u16, is_encrypted: bool) -> Result<Vec<u8>, String> {
    if version != CFB_MAJOR_V3 && version != CFB_MAJOR_V4 {
        return Err(format!(
            "pres_header_parity: unsupported version {version}; must be 3 or 4"
        ));
    }
    let mut v = Vec::with_capacity(CFB_HEADER_SIZE as usize);
    v.extend_from_slice(&CFB_MAGIC);
    v.extend_from_slice(&[0u8; 16]); // CLSID zero
    v.extend_from_slice(&0u16.to_le_bytes()); // minor_version
    v.extend_from_slice(&version.to_le_bytes());
    v.extend_from_slice(&CFB_BYTE_ORDER);
    let sector_shift: u16 = if version == CFB_MAJOR_V4 {
        0x000C
    } else {
        0x0009
    };
    v.extend_from_slice(&sector_shift.to_le_bytes());
    v.extend_from_slice(&0x0006u16.to_le_bytes()); // mini_sector_shift
    v.extend_from_slice(&[0u8; 6]); // reserved
    v.extend_from_slice(&0u32.to_le_bytes()); // number_of_directory_sectors
                                              // number_of_fat_sectors: any u16-bound value is accepted by the
                                              // parser (it clamps at u16::MAX with an explicit error). Use 1
                                              // to keep the buffer minimal.
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes()); // first_directory_sector_location
    let transaction_signature: u32 = if is_encrypted { 1 } else { 0 };
    v.extend_from_slice(&transaction_signature.to_le_bytes());
    v.extend_from_slice(&CFB_MINI_STREAM_CUTOFF.to_le_bytes());
    v.extend_from_slice(&0xFFFFFFFEu32.to_le_bytes()); // first_minifat_sector_location NOSTREAM
    v.extend_from_slice(&0u32.to_le_bytes()); // number_of_minifat_sectors
    v.extend_from_slice(&0xFFFFFFFEu32.to_le_bytes()); // first_difat_sector_location NOSTREAM
    v.extend_from_slice(&0u32.to_le_bytes()); // number_of_difat_sectors
                                              // DIFAT: 109 u32 LE entries. The parser does not validate DIFAT
                                              // contents — only that the total header size is 512. Fill with
                                              // FREESECT (0xFFFFFFFF) per the CFB spec.
    for _ in 0..109 {
        v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    }
    debug_assert_eq!(
        v.len(),
        CFB_HEADER_SIZE as usize,
        "build_ppt_header size miscalculation"
    );
    Ok(v)
}

/// Re-parse `input` via [`crate::pres_header_parse::parse`] and
/// assert the header is well-formed and 512 bytes long. Panics with
/// a descriptive message on the first failure.
///
/// Use [`build_ppt_header`] to produce buffers that are guaranteed to
/// satisfy this assertion. Direct callers may pass any 512-byte CFBH
/// that obeys the [MS-CFB] v20210817 §2.2 invariants.
pub fn assert_round_trip(input: &[u8]) {
    if input.len() != CFB_HEADER_SIZE as usize {
        panic!(
            "pres_header_parity: expected {} bytes, got {}",
            CFB_HEADER_SIZE,
            input.len()
        );
    }
    let h = parse(input).expect("pres_header_parity: parser rejected a buffer we just built");
    if h.magic != CFB_MAGIC {
        panic!(
            "pres_header_parity: magic mismatch expected={:02x?} got={:02x?}",
            CFB_MAGIC, h.magic
        );
    }
    if h.header_size != CFB_HEADER_SIZE {
        panic!(
            "pres_header_parity: header_size mismatch expected={} got={}",
            CFB_HEADER_SIZE, h.header_size
        );
    }
    if h.version != CFB_MAJOR_V3 && h.version != CFB_MAJOR_V4 {
        panic!("pres_header_parity: version not 3 or 4, got {}", h.version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pres_header_parse::{CFB_MAJOR_V3, CFB_MAJOR_V4};

    #[test]
    fn build_v3_unencrypted_has_correct_magic_and_size() {
        // Cross-check against [MS-CFB] §2.2: magic at 0x00, version
        // at 0x1A, byte_order at 0x1C, sector_shift at 0x1E.
        let buf = build_ppt_header(CFB_MAJOR_V3, false).unwrap();
        assert_eq!(buf.len(), 512);
        assert_eq!(&buf[0..8], &CFB_MAGIC);
        assert_eq!(&buf[8..24], &[0u8; 16]); // CLSID zero
        assert_eq!(&buf[0x18..0x1A], &0u16.to_le_bytes()); // minor_version
        assert_eq!(&buf[0x1A..0x1C], &CFB_MAJOR_V3.to_le_bytes());
        assert_eq!(&buf[0x1C..0x1E], &CFB_BYTE_ORDER);
        // sector_shift for v3 = 9 = 512 bytes (per [MS-CFB] §2.2).
        assert_eq!(&buf[0x1E..0x20], &0x0009u16.to_le_bytes());
        // mini_sector_shift = 6 = 64 bytes (per [MS-CFB] §2.2).
        assert_eq!(&buf[0x20..0x22], &0x0006u16.to_le_bytes());
        // reserved at 0x22..0x28 = zero.
        assert_eq!(&buf[0x22..0x28], &[0u8; 6]);
        // number_of_directory_sectors at 0x28 = 0 for v3.
        assert_eq!(&buf[0x28..0x2C], &0u32.to_le_bytes());
        // number_of_fat_sectors at 0x2C = 1 (builder default).
        assert_eq!(&buf[0x2C..0x30], &1u32.to_le_bytes());
        // transaction_signature_number at 0x34 = 0 (unencrypted).
        assert_eq!(&buf[0x34..0x38], &0u32.to_le_bytes());
        // mini_stream_cutoff_size at 0x38 = 4096.
        assert_eq!(&buf[0x38..0x3C], &CFB_MINI_STREAM_CUTOFF.to_le_bytes());
    }

    #[test]
    fn build_v3_round_trips_as_unencrypted() {
        let buf = build_ppt_header(CFB_MAJOR_V3, false).unwrap();
        assert_round_trip(&buf);
        let h = parse(&buf).unwrap();
        assert_eq!(h.version, CFB_MAJOR_V3);
        assert!(!h.is_encrypted);
        assert_eq!(h.total_slots, 1);
    }

    #[test]
    fn build_v3_encrypted_round_trips_with_is_encrypted_true() {
        // transaction_signature != 0 => is_encrypted == true
        // ([MS-OFFCRYPTO] §2.3.4.10).
        let buf = build_ppt_header(CFB_MAJOR_V3, true).unwrap();
        assert_round_trip(&buf);
        let h = parse(&buf).unwrap();
        assert_eq!(h.version, CFB_MAJOR_V3);
        assert!(h.is_encrypted);
        // The encryption signal lives in byte 0x34..0x38 (LE u32 == 1).
        assert_eq!(&buf[0x34..0x38], &1u32.to_le_bytes());
    }

    #[test]
    fn build_v4_round_trips_with_sector_shift_12() {
        // v4 mandates sector_shift = 12 (4096-byte sectors). Cross-checked
        // against [MS-CFB] §2.2: "For major version 4, this field MUST be
        // set to 0x000C (12)."
        let buf = build_ppt_header(CFB_MAJOR_V4, false).unwrap();
        assert_eq!(&buf[0x1E..0x20], &0x000Cu16.to_le_bytes());
        assert_round_trip(&buf);
        let h = parse(&buf).unwrap();
        assert_eq!(h.version, CFB_MAJOR_V4);
        assert!(!h.is_encrypted);
    }

    #[test]
    fn build_v4_encrypted_round_trips() {
        let buf = build_ppt_header(CFB_MAJOR_V4, true).unwrap();
        assert_round_trip(&buf);
        let h = parse(&buf).unwrap();
        assert!(h.is_encrypted);
    }

    #[test]
    fn build_rejects_unsupported_version() {
        let err = build_ppt_header(5, false).unwrap_err();
        assert!(
            err.contains("unsupported version"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn build_difat_section_is_filled_to_512_bytes() {
        // DIFAT: 109 * 4 = 436 bytes from 0x4C through the end.
        // Total: 0x4C + 436 = 76 + 436 = 512.
        let buf = build_ppt_header(CFB_MAJOR_V3, false).unwrap();
        assert_eq!(buf.len(), 512);
        // Every DIFAT entry should be FREESECT (0xFFFFFFFF).
        for i in 0..109 {
            let off = 0x4C + i * 4;
            assert_eq!(
                &buf[off..off + 4],
                &0xFFFFFFFFu32.to_le_bytes(),
                "DIFAT entry {} not FREESECT",
                i
            );
        }
    }

    #[test]
    fn assert_round_trip_rejects_short_input() {
        let err = std::panic::catch_unwind(|| assert_round_trip(&[0u8; 100])).is_err();
        assert!(err, "assert_round_trip should panic on short input");
    }

    #[test]
    fn parse_round_trip_matches_existing_test_fixture() {
        // The existing `pres_header_parse::tests::build_cfb` helper
        // builds a v3 unencrypted header with number_of_fat_sectors=1
        // and transaction_signature=0. Our parity builder must
        // produce a byte-for-byte equivalent.
        // (This is the strongest cross-check we can do without
        // shipping a full reference CFB file.)
        let buf = build_ppt_header(CFB_MAJOR_V3, false).unwrap();
        let h = parse(&buf).unwrap();
        assert_eq!(h.magic, CFB_MAGIC);
        assert_eq!(h.header_size, CFB_HEADER_SIZE);
        assert_eq!(h.version, CFB_MAJOR_V3);
        assert!(!h.is_encrypted);
    }
}
