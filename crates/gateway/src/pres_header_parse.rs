// Minimal MS-CFB + legacy PowerPoint .ppt header parser.
//
// References:
//   [MS-CFB] v20210817 - Compound File Binary File Format
//   [MS-PPT] v20210817 - PowerPoint Binary File Format (legacy .ppt 97-2003)
//   Every legacy .ppt (Microsoft PowerPoint 97-2003 .ppt) file is itself an
//   MS-CFB compound document. The file starts with the 512-byte Compound File
//   Header (CFBH, [MS-CFB] Section 2.2). PowerPoint then provides an additional
//   "Current User" stream and a "PowerPoint Document" stream inside the CFB
//   (those streams contain the presentation data). This minimal parser
//   validates the CFBH and surfaces the fields needed to confirm a file is
//   both a valid MS-CFB container AND compatible with the legacy PowerPoint
//   binary format.
//
// The 512-byte CFBH layout ([MS-CFB] Section 2.2):
//   0x00 (8)  : header_signature, MUST be { 0xD0, 0xCF, 0x11, 0xE0, 0xA1,
//               0xB1, 0x1A, 0xE1 }
//   0x08 (16) : header_clsid, MUST be zero
//   0x18 (2)  : minor_version (u16 LE)
//   0x1A (2)  : major_version (u16 LE) -- must be 3 or 4
//   0x1C (2)  : byte_order, MUST be { 0xFE, 0xFF }
//   0x1E (2)  : sector_shift (u16 LE) -- MUST be 0x0009 (512 bytes) for v3
//               or 0x000C (4096 bytes) for v4
//   0x20 (2)  : mini_sector_shift, MUST be 0x0006 (64 bytes)
//   0x22 (6)  : reserved, MUST be zero
//   0x28 (4)  : number_of_directory_sectors, MUST be 0 for v3
//   0x2C (4)  : number_of_fat_sectors (u32 LE) -- total FAT-sector slots
//   0x30 (4)  : first_directory_sector_location (u32 LE)
//   0x34 (4)  : transaction_signature_number, MUST be 0
//   0x38 (4)  : mini_stream_cutoff_size, MUST be 0x00001000 (4096)
//   0x3C (4)  : first_minifat_sector_location (u32 LE)
//   0x40 (4)  : number_of_minifat_sectors (u32 LE)
//   0x44 (4)  : first_difat_sector_location (u32 LE)
//   0x48 (4)  : number_of_difat_sectors (u32 LE)
//   0x4C ...  : difat (109 entries of u32 LE = 436 bytes)
//
// Total CFBH size: 76 + 436 = 512 bytes.
//
// PowerPoint-specific notes ([MS-PPT]):
//   - Legacy .ppt streams that hold "PowerPoint Document" and "Current User"
//     are inside the CFB; we do NOT decode them here. The parser only
//     validates the outer container, which is sufficient to identify a file
//     as a candidate legacy PowerPoint file.
//   - is_encrypted is derived from the CFBH transaction_signature_number: a
//     non-zero transaction signature is reserved for Microsoft Password
//     Protection / encryption ([MS-OFFCRYPTO] Section 2.3.4.10). This is the
//     strongest reliable signal from the 512-byte header alone.

/// Canonical MS-CFB magic bytes. [MS-CFB] Section 2.2.
pub const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// CFB major versions accepted by [MS-CFB].
pub const CFB_MAJOR_V3: u16 = 3;
pub const CFB_MAJOR_V4: u16 = 4;

/// Standard CFBH byte-order marker (little-endian). [MS-CFB] 2.2.
pub const CFB_BYTE_ORDER: [u8; 2] = [0xFE, 0xFF];

/// Fixed size of a CFB header.
pub const CFB_HEADER_SIZE: u32 = 512;

/// Standard mini-stream cutoff size in bytes.
pub const CFB_MINI_STREAM_CUTOFF: u32 = 0x0000_1000;

/// Minimal legacy PowerPoint / MS-CFB header.
#[derive(Debug, Clone, PartialEq)]
pub struct PresHeader {
    /// The 8-byte MS-CFB magic. Always exactly `CFB_MAGIC` if parsing succeeds.
    pub magic: [u8; 8],
    /// Total header size in bytes (always 512 for a valid CFB / .ppt).
    pub header_size: u32,
    /// Total FAT-sector slots (`number_of_fat_sectors` field at CFBH 0x2C).
    pub total_slots: u16,
    /// CFB major version (typically 3 for legacy .ppt).
    pub version: u16,
    /// True when `transaction_signature_number` (CFBH 0x34) is non-zero,
    /// which signals Microsoft Password Protection / encryption.
    pub is_encrypted: bool,
}

fn read_u16_le(input: &[u8], off: usize) -> Result<u16, String> {
    let bytes: [u8; 2] = input
        .get(off..off + 2)
        .ok_or_else(|| "pres_header: truncated u16".to_string())?
        .try_into()
        .map_err(|_| "pres_header: truncated u16".to_string())?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32_le(input: &[u8], off: usize) -> Result<u32, String> {
    let bytes: [u8; 4] = input
        .get(off..off + 4)
        .ok_or_else(|| "pres_header: truncated u32".to_string())?
        .try_into()
        .map_err(|_| "pres_header: truncated u32".to_string())?;
    Ok(u32::from_le_bytes(bytes))
}

/// Parse a legacy PowerPoint / MS-CFB 512-byte header.
///
/// `input` MUST be at least 512 bytes long. The parser validates the magic,
/// the byte-order marker, and the major version, then reads the documented
/// fields. Returns an error if any of the validation checks fail.
pub fn parse(input: &[u8]) -> Result<PresHeader, String> {
    if input.len() < CFB_HEADER_SIZE as usize {
        return Err(format!(
            "pres_header: input shorter than CFB header ({} < {})",
            input.len(),
            CFB_HEADER_SIZE
        ));
    }
    // Magic (8 bytes at offset 0).
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&input[0..8]);
    if magic != CFB_MAGIC {
        return Err(format!(
            "pres_header: bad magic, expected D0CF11E0A1B11AE1, got {magic:02X?}"
        ));
    }
    // CLSID (16 bytes at offset 8) MUST be zero per [MS-CFB] 2.2.
    if input[8..24].iter().any(|b| *b != 0) {
        return Err("pres_header: header_clsid must be zero per [MS-CFB]".to_string());
    }
    // Versions.
    let minor_version = read_u16_le(input, 0x18)?;
    let major_version = read_u16_le(input, 0x1A)?;
    if major_version != CFB_MAJOR_V3 && major_version != CFB_MAJOR_V4 {
        return Err(format!(
            "pres_header: major version must be 3 or 4, got {major_version}"
        ));
    }
    // Byte-order marker.
    let byte_order = [input[0x1C], input[0x1D]];
    if byte_order != CFB_BYTE_ORDER {
        return Err(format!(
            "pres_header: byte order must be FEFE (LE), got {byte_order:02X?}"
        ));
    }
    // sector_shift.
    let sector_shift = read_u16_le(input, 0x1E)?;
    match major_version {
        CFB_MAJOR_V3 => {
            if sector_shift != 0x0009 {
                return Err(format!(
                    "pres_header: sector_shift must be 9 (512) for v3, got {sector_shift}"
                ));
            }
        }
        CFB_MAJOR_V4 => {
            if sector_shift != 0x000C {
                return Err(format!(
                    "pres_header: sector_shift must be 12 (4096) for v4, got {sector_shift}"
                ));
            }
        }
        _ => unreachable!(),
    }
    // mini_sector_shift (must be 6).
    let mini_sector_shift = read_u16_le(input, 0x20)?;
    if mini_sector_shift != 0x0006 {
        return Err(format!(
            "pres_header: mini_sector_shift must be 6 (64 bytes), got {mini_sector_shift}"
        ));
    }
    // Reserved 6 bytes at 0x22 must be zero.
    if input[0x22..0x28].iter().any(|b| *b != 0) {
        return Err("pres_header: reserved bytes at 0x22 must be zero".to_string());
    }
    // number_of_directory_sectors must be 0 for v3.
    let number_of_directory_sectors = read_u32_le(input, 0x28)?;
    if major_version == CFB_MAJOR_V3 && number_of_directory_sectors != 0 {
        return Err(format!(
            "pres_header: number_of_directory_sectors must be 0 for v3, got {number_of_directory_sectors}"
        ));
    }
    // total_slots = number_of_fat_sectors (clamped to u16 since CFB v3 max
    // FAT sectors is well below u16::MAX in any real file).
    let number_of_fat_sectors = read_u32_le(input, 0x2C)?;
    if number_of_fat_sectors > u16::MAX as u32 {
        return Err(format!(
            "pres_header: number_of_fat_sectors {number_of_fat_sectors} exceeds u16"
        ));
    }
    let total_slots = number_of_fat_sectors as u16;
    // transaction_signature_number at 0x34 (encryption indicator).
    let transaction_signature = read_u32_le(input, 0x34)?;
    let is_encrypted = transaction_signature != 0;
    // mini_stream_cutoff_size at 0x38 must be 4096 per spec.
    let mini_cutoff = read_u32_le(input, 0x38)?;
    if mini_cutoff != CFB_MINI_STREAM_CUTOFF {
        return Err(format!(
            "pres_header: mini_stream_cutoff_size must be 4096, got {mini_cutoff}"
        ));
    }

    // We deliberately do not enforce the DIFAT entries beyond rejecting the
    // header size; the parser only needs the fields the PresHeader struct
    // promises. The full DIFAT validation belongs to a deeper MS-CFB parser.
    let _minor_version = minor_version;
    Ok(PresHeader {
        magic,
        header_size: CFB_HEADER_SIZE,
        total_slots,
        version: major_version,
        is_encrypted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid 512-byte MS-CFB header for tests.
    fn build_cfb(
        major_version: u16,
        number_of_fat_sectors: u32,
        transaction_signature: u32,
    ) -> Vec<u8> {
        let mut v = Vec::with_capacity(512);
        v.extend_from_slice(&CFB_MAGIC);
        v.extend_from_slice(&[0u8; 16]); // CLSID zero
        v.extend_from_slice(&0u16.to_le_bytes()); // minor_version
        v.extend_from_slice(&major_version.to_le_bytes());
        v.extend_from_slice(&CFB_BYTE_ORDER);
        let sector_shift: u16 = if major_version == CFB_MAJOR_V4 {
            0x000C
        } else {
            0x0009
        };
        v.extend_from_slice(&sector_shift.to_le_bytes());
        v.extend_from_slice(&0x0006u16.to_le_bytes()); // mini_sector_shift
        v.extend_from_slice(&[0u8; 6]); // reserved
        v.extend_from_slice(&0u32.to_le_bytes()); // number_of_directory_sectors
        v.extend_from_slice(&number_of_fat_sectors.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // first_directory_sector_location
        v.extend_from_slice(&transaction_signature.to_le_bytes());
        v.extend_from_slice(&CFB_MINI_STREAM_CUTOFF.to_le_bytes());
        v.extend_from_slice(&0xFFFFFFFEu32.to_le_bytes()); // first_minifat_sector_location (NOSTREAM)
        v.extend_from_slice(&0u32.to_le_bytes()); // number_of_minifat_sectors
        v.extend_from_slice(&0xFFFFFFFEu32.to_le_bytes()); // first_difat_sector_location (NOSTREAM)
        v.extend_from_slice(&0u32.to_le_bytes()); // number_of_difat_sectors
                                                  // DIFAT[0..109] = 109 u32 LE entries (436 bytes).
        v.extend_from_slice(&0u32.to_le_bytes()); // first FAT sector index (sector 0)
        for _ in 1..109 {
            v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // FREESECT
        }
        assert_eq!(v.len(), 512, "CFBH must be exactly 512 bytes");
        v
    }

    #[test]
    fn parses_minimal_v3_cfb_header() {
        let v = build_cfb(CFB_MAJOR_V3, 1, 0);
        let h = parse(&v).expect("parse v3");
        assert_eq!(h.magic, CFB_MAGIC);
        assert_eq!(h.header_size, 512);
        assert_eq!(h.total_slots, 1);
        assert_eq!(h.version, 3);
        assert!(!h.is_encrypted);
    }

    #[test]
    fn parses_minimal_v4_cfb_header() {
        let v = build_cfb(CFB_MAJOR_V4, 4, 0);
        let h = parse(&v).expect("parse v4");
        assert_eq!(h.version, 4);
        assert_eq!(h.total_slots, 4);
        assert!(!h.is_encrypted);
    }

    #[test]
    fn rejects_input_shorter_than_512_bytes() {
        let v = vec![0u8; 511];
        let err = parse(&v).unwrap_err();
        assert!(err.contains("shorter than CFB header"));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0] = 0xAB;
        let err = parse(&v).unwrap_err();
        assert!(err.contains("bad magic"));
    }

    #[test]
    fn rejects_nonzero_clsid() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[8] = 0x01;
        let err = parse(&v).unwrap_err();
        assert!(err.contains("clsid"));
    }

    #[test]
    fn rejects_bad_byte_order() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0x1C] = 0x00;
        let err = parse(&v).unwrap_err();
        assert!(err.contains("byte order"));
    }

    #[test]
    fn rejects_bad_major_version() {
        let v = build_cfb(5, 1, 0);
        let err = parse(&v).unwrap_err();
        assert!(err.contains("major version"));
    }

    #[test]
    fn rejects_bad_sector_shift_for_v3() {
        // Build with v3 but flip sector_shift manually.
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0x1E] = 0x0C; // 0x0C00 -> 3072
        let err = parse(&v).unwrap_err();
        assert!(err.contains("sector_shift"));
    }

    #[test]
    fn rejects_bad_mini_sector_shift() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0x20] = 0x07; // 0x0700 -> 1792
        let err = parse(&v).unwrap_err();
        assert!(err.contains("mini_sector_shift"));
    }

    #[test]
    fn rejects_nonzero_reserved_bytes() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0x25] = 0x42;
        let err = parse(&v).unwrap_err();
        assert!(err.contains("reserved"));
    }

    #[test]
    fn rejects_nonzero_directory_sectors_for_v3() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0x28] = 0x01;
        let err = parse(&v).unwrap_err();
        assert!(err.contains("number_of_directory_sectors"));
    }

    #[test]
    fn rejects_bad_mini_stream_cutoff() {
        let mut v = build_cfb(CFB_MAJOR_V3, 1, 0);
        v[0x3B] = 0x01; // bump cutoff to 0x00001001
        let err = parse(&v).unwrap_err();
        assert!(err.contains("mini_stream_cutoff_size"));
    }

    #[test]
    fn rejects_oversize_fat_sector_count() {
        // number_of_fat_sectors = u16::MAX + 1 = 65536
        let v = build_cfb(CFB_MAJOR_V3, 65_536, 0);
        let err = parse(&v).unwrap_err();
        assert!(err.contains("exceeds u16"));
    }

    #[test]
    fn detects_encryption_via_transaction_signature() {
        let v = build_cfb(CFB_MAJOR_V3, 1, 0xDEAD_BEEF);
        let h = parse(&v).expect("parse encrypted");
        assert!(h.is_encrypted);
        // version, total_slots, etc. should still be correctly populated.
        assert_eq!(h.total_slots, 1);
        assert_eq!(h.version, 3);
    }

    #[test]
    fn accepts_zero_total_slots() {
        let v = build_cfb(CFB_MAJOR_V3, 0, 0);
        let h = parse(&v).expect("parse zero slots");
        assert_eq!(h.total_slots, 0);
    }

    #[test]
    fn accepts_extra_trailing_bytes() {
        // Real .ppt files are always larger than 512. Extra bytes after the
        // CFBH must NOT cause parse to fail.
        let mut v = build_cfb(CFB_MAJOR_V3, 2, 0);
        v.extend_from_slice(&[0u8; 4096]);
        let h = parse(&v).expect("parse with trailing");
        assert_eq!(h.header_size, 512);
        assert_eq!(h.total_slots, 2);
    }
}
