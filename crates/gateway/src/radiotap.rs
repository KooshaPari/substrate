// Minimal radiotap header parser/encoder. Radiotap is a "field-extensible"
// 802.11 metadata header laid out as:
//
//   0                   1                   2                   3
//   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |  Version (0)  |     pad (0)   |       Header Length           |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |          Present flags (u32 little-endian)                    |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |          Optional fields (bit-set ordered, see below)        |
//  ~                                                               ~
//  |                                                               |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// The "present flags" field is laid out so that bit N (LSB=0) indicates
// whether the Nth defined field is present in the header. Each present
// field occupies its own aligned slot in the order defined by the
// canonical radiotap field-numbering scheme (TSFT=0, Flags=1, ...).
//
// Field sizes/alignments (per radiotap.org/fields/defined):
//
//   Bit  Field                       Structure           Align  Size
//   ---  --------------------------  ------------------  -----  ----
//    0   TSFT                        u64                  8      8
//    1   Flags                       u8                   1      1
//    2   Rate                        u8 (500 kbps units)  1      1
//    3   Channel                     u16 freq, u16 flags  2      4
//    4   FHSS                        u8 hop_set, u8 pat   1      2
//    5   Antenna signal              s8 dBm               1      1
//    6   Antenna noise               s8 dBm               1      1
//    7   Lock quality                u16                  2      2
//    8   TX attenuation              u16 (dB)             2      2
//    9   dB TX attenuation           u16                  2      2
//   10   dBm TX power                s8                   1      1
//   11   Antenna                     u8                   1      1
//   12   dB antenna signal           u8                   1      1
//   13   dB antenna noise            u8                   1      1
//
// This module supports the first 14 fields (most commonly captured by
// tcpdump/wireshark). Extending to VHT/HE/etc. requires additional
// per-field structs and is out of scope here.

pub const RADIOTAP_VERSION: u8 = 0;

pub const FIELD_TSFT: u32 = 1 << 0;
pub const FIELD_FLAGS: u32 = 1 << 1;
pub const FIELD_RATE: u32 = 1 << 2;
pub const FIELD_CHANNEL: u32 = 1 << 3;
pub const FIELD_FHSS: u32 = 1 << 4;
pub const FIELD_ANTENNA_SIGNAL: u32 = 1 << 5;
pub const FIELD_ANTENNA_NOISE: u32 = 1 << 6;
pub const FIELD_LOCK_QUALITY: u32 = 1 << 7;
pub const FIELD_TX_ATTENUATION: u32 = 1 << 8;
pub const FIELD_DB_TX_ATTENUATION: u32 = 1 << 9;
pub const FIELD_DBM_TX_POWER: u32 = 1 << 10;
pub const FIELD_ANTENNA: u32 = 1 << 11;
pub const FIELD_DB_ANTENNA_SIGNAL: u32 = 1 << 12;
pub const FIELD_DB_ANTENNA_NOISE: u32 = 1 << 13;

/// Field layout (size in bytes, alignment requirement).
struct FieldSpec {
    size: usize,
    align: usize,
}

const SPECS: [FieldSpec; 14] = [
    FieldSpec { size: 8, align: 8 },  // TSFT
    FieldSpec { size: 1, align: 1 },  // Flags
    FieldSpec { size: 1, align: 1 },  // Rate
    FieldSpec { size: 4, align: 2 },  // Channel
    FieldSpec { size: 2, align: 1 },  // FHSS
    FieldSpec { size: 1, align: 1 },  // Antenna signal
    FieldSpec { size: 1, align: 1 },  // Antenna noise
    FieldSpec { size: 2, align: 2 },  // Lock quality
    FieldSpec { size: 2, align: 2 },  // TX attenuation
    FieldSpec { size: 2, align: 2 },  // dB TX attenuation
    FieldSpec { size: 1, align: 1 },  // dBm TX power
    FieldSpec { size: 1, align: 1 },  // Antenna
    FieldSpec { size: 1, align: 1 },  // dB antenna signal
    FieldSpec { size: 1, align: 1 },  // dB antenna noise
];

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RadiotapHeader {
    pub version: u8,
    pub pad_byte: u8,
    pub length: u16,
    pub present: u32,
    pub body: Vec<u8>,
}

/// Parse a radiotap header from the input bytes. The returned
/// `body` covers the field area (everything past the 8-byte fixed
/// prefix). Decoding of individual fields is the caller's job via
/// `read_u8/read_u16_le/read_u64_le` at the offsets implied by the
/// canonical field layout below.
pub fn parse(input: &[u8]) -> Result<RadiotapHeader, String> {
    if input.len() < 8 {
        return Err(format!(
            "radiotap too short: need at least 8 bytes, got {}",
            input.len()
        ));
    }
    let version = input[0];
    let pad_byte = input[1];
    let length = u16::from_le_bytes([input[2], input[3]]);
    let present = u32::from_le_bytes([input[4], input[5], input[6], input[7]]);
    if input.len() < usize::from(length) {
        return Err(format!(
            "radiotap length {length} exceeds available {} bytes",
            input.len()
        ));
    }
    Ok(RadiotapHeader {
        version,
        pad_byte,
        length,
        present,
        body: input[8..usize::from(length)].to_vec(),
    })
}

/// Walk the canonical field list and advance `offset` by each present
/// field's aligned size. Returns the offsets of fields that the caller
/// cared about. Use `for_each_field` for a simpler walk.
pub fn for_each_field<F: FnMut(usize, usize, u32)>(
    present: u32,
    mut cb: F,
) {
    let mut offset = 0usize;
    for (idx, spec) in SPECS.iter().enumerate() {
        let bit = 1u32 << idx;
        if present & bit == 0 {
            continue;
        }
        let aligned = (offset + spec.align - 1) & !(spec.align - 1);
        cb(aligned, spec.size, bit);
        offset = aligned + spec.size;
    }
}

/// Encode a radiotap header from the fields present in `present`.
/// `body` should be the raw field bytes (already padded to the
/// alignment requirements of the canonical field ordering).
pub fn encode(present: u32, body: &[u8]) -> Vec<u8> {
    // The "length" field encodes the total header length including the
    // fixed 8-byte prefix. Round the body up to align with the next
    // 4-byte boundary that puts the header on a 32-bit boundary (most
    // drivers require this).
    let total_unaligned = 8 + body.len();
    let pad_count = (4 - (total_unaligned % 4)) % 4;
    let length = (total_unaligned + pad_count) as u16;
    let mut out = Vec::with_capacity(length as usize);
    out.push(RADIOTAP_VERSION);
    out.push(0); // pad byte
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&present.to_le_bytes());
    out.extend_from_slice(body);
    out.resize(length as usize, 0);
    out
}

/// Compute the offset (relative to start of header body) of the Nth
/// field in the present mask, plus its size. Returns
/// `Err(String)` if the field is not present or is out of the
/// supported range.
pub fn field_offset(
    present: u32,
    field_bit: u32,
) -> Result<(usize, usize), String> {
    if field_bit == 0 || field_bit.trailing_zeros() >= SPECS.len() as u32 {
        return Err(format!("unsupported field bit 0x{field_bit:x}"));
    }
    if present & field_bit == 0 {
        return Err(format!("field bit 0x{field_bit:x} not present"));
    }
    let mut offset = 0usize;
    for (idx, spec) in SPECS.iter().enumerate() {
        let bit = 1u32 << idx;
        if present & bit == 0 {
            continue;
        }
        let aligned = (offset + spec.align - 1) & !(spec.align - 1);
        if bit == field_bit {
            return Ok((aligned, spec.size));
        }
        offset = aligned + spec.size;
    }
    Err(format!("field bit 0x{field_bit:x} not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid header: version=0, pad=0, length=8, present=0.
    /// This is the exact header a radiotap driver emits when no fields
    /// are present (e.g. before any monitor-mode filter applies).
    #[test]
    fn parse_minimal() {
        let bytes = vec![0, 0, 8, 0, 0, 0, 0, 0];
        let hdr = parse(&bytes).unwrap();
        assert_eq!(hdr.version, 0);
        assert_eq!(hdr.pad_byte, 0);
        assert_eq!(hdr.length, 8);
        assert_eq!(hdr.present, 0);
        assert!(hdr.body.is_empty());
    }

    #[test]
    fn parse_too_short() {
        let bytes = vec![0, 0, 8];
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn parse_length_exceeds_input() {
        let bytes = vec![0, 0, 100, 0, 0, 0, 0, 0];
        assert!(parse(&bytes).is_err());
    }

    /// Header with present=FLAGS|RATE|ANTENNA_SIGNAL (bits 1,2,5).
    /// Expected offsets: 0 (Flags,1B), 1 (Rate,1B), 2 (Antenna,1B).
    #[test]
    fn field_offsets_simple() {
        let present = FIELD_FLAGS | FIELD_RATE | FIELD_ANTENNA_SIGNAL;
        let (off_flags, sz_flags) = field_offset(present, FIELD_FLAGS).unwrap();
        assert_eq!((off_flags, sz_flags), (0, 1));
        let (off_rate, sz_rate) = field_offset(present, FIELD_RATE).unwrap();
        assert_eq!((off_rate, sz_rate), (1, 1));
        let (off_sig, sz_sig) = field_offset(present, FIELD_ANTENNA_SIGNAL).unwrap();
        assert_eq!((off_sig, sz_sig), (2, 1));
    }

    /// Header with TSFT (u64, 8-byte aligned). With TSFT alone the
    /// field starts at offset 0. With TSFT + Channel, the Channel is
    /// at offset 8 (TSFT-aligned) and ends at 8+4=12.
    #[test]
    fn field_offset_tsft_alignment() {
        let present = FIELD_TSFT | FIELD_CHANNEL;
        let (off_tsft, sz_tsft) = field_offset(present, FIELD_TSFT).unwrap();
        assert_eq!((off_tsft, sz_tsft), (0, 8));
        let (off_ch, sz_ch) = field_offset(present, FIELD_CHANNEL).unwrap();
        assert_eq!((off_ch, sz_ch), (8, 4));
    }

    /// A TSFT + Antenna signal + dBm TX power header. Antenna signal
    /// has alignment 1 (TSFT is 8-byte aligned so off=8, Antenna
    /// starts immediately at 8+1=... wait — TSFT is 8B ending at 8,
    /// Antenna at 8, dBm at 9). Verify.
    #[test]
    fn field_offset_mixed_alignment() {
        let present = FIELD_TSFT | FIELD_ANTENNA_SIGNAL | FIELD_DBM_TX_POWER;
        let (_, sz_tsft) = field_offset(present, FIELD_TSFT).unwrap();
        assert_eq!(sz_tsft, 8);
        let (off_sig, sz_sig) = field_offset(present, FIELD_ANTENNA_SIGNAL).unwrap();
        assert_eq!((off_sig, sz_sig), (8, 1));
        let (off_pwr, sz_pwr) = field_offset(present, FIELD_DBM_TX_POWER).unwrap();
        assert_eq!((off_pwr, sz_pwr), (9, 1));
    }

    /// Round-trip: encode a minimal present=0 header, parse it back.
    #[test]
    fn round_trip_minimal() {
        let bytes = encode(0, &[]);
        assert_eq!(bytes, vec![0u8, 0, 8, 0, 0, 0, 0, 0]);
        let hdr = parse(&bytes).unwrap();
        assert_eq!(hdr.present, 0);
        assert_eq!(hdr.length, 8);
    }

    /// Encode + parse a Flags+Rate header and verify the body carries
    /// the values through. We don't decode individual fields here;
    /// `field_offset` + body slicing is the caller's job.
    #[test]
    fn round_trip_with_body() {
        let present = FIELD_FLAGS | FIELD_RATE;
        let body = vec![0x12, 0x18]; // flags=0x12, rate=0x18 (24 * 500 kbps = 12 Mbps)
        let bytes = encode(present, &body);
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[1], 0);
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);
        assert_eq!(length as usize, bytes.len());
        let hdr = parse(&bytes).unwrap();
        assert_eq!(hdr.present, present);
        // Body length is padded up to a 4-byte boundary by `encode`,
        // so a 2-byte payload becomes 4 bytes in the parsed body.
        assert_eq!(hdr.body.len(), 4);
        assert_eq!(&hdr.body[..2], &body[..]);
    }

    #[test]
    fn field_not_present() {
        let present = FIELD_FLAGS;
        assert!(field_offset(present, FIELD_RATE).is_err());
    }

    #[test]
    fn for_each_visits_in_order() {
        let present = FIELD_FLAGS | FIELD_RATE | FIELD_CHANNEL;
        let mut visited = Vec::new();
        for_each_field(present, |off, sz, bit| {
            visited.push((off, sz, bit));
        });
        assert_eq!(visited.len(), 3);
        assert_eq!(visited[0], (0, 1, FIELD_FLAGS));
        assert_eq!(visited[1], (1, 1, FIELD_RATE));
        // Channel needs 2-byte alignment, so it sits at offset 2.
        assert_eq!(visited[2], (2, 4, FIELD_CHANNEL));
    }
}
