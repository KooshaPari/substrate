// Minimal NTP control message codec (RFC 1305 §8, Mode 6).
//
// An NTP control message is exactly 48 bytes of fixed header followed
// by an optional data payload. The header layout (all fields
// big-endian) is:
//
//   off  size  field
//   ---  ----  ---------------------------------------------------
//    0    1   LI (2 bits) | VN (3 bits) | Mode (3 bits)
//    1    1   Response (1) | More (1) | Error (1) | Opcode (5)
//    2    2   Sequence
//    4    2   Status
//    6    2   Association ID
//    8    2   Offset
//   10    4   Count
//   14   32   Data (MBZ for requests; response-specific otherwise)
//
// In practice the "official" RFC 1305 §8 wire layout uses the
// extended 12-field NTP header style (same first 8 bytes as the
// timestamp header) plus a 4-byte payload-length hint and a variable
// payload. The simpler Mode-6 framing used by `ntpq` and many
// implementations is:
//
//   0..4   : LI/VN/Mode/Response/Error/More/Opcode (4 bytes)
//   4..8   : Sequence (2), Status (2)
//   8..12  : Association ID (2), Offset (2)
//   12..16 : Count (4)
//   16..48 : Reserved (32 bytes)
//   48..   : Variable-length data payload
//
// We follow that simpler framing here. VN must be 2 or 3; Mode must
// be 6 (control). Response, Error, More are 1-bit flags.

/// Required header length in bytes.
pub const CTRL_HEADER_LEN: usize = 48;

/// Mode-6 magic. VN << 3 | Mode packed into the first byte.
pub const CTRL_VN_MODE: u8 = (2u8 << 3) | 6u8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtrlMsg {
    /// True if this is a response, false if a request.
    pub response: bool,
    /// True if the response carries an error code.
    pub error: bool,
    /// True if more fragments follow this one.
    pub more: bool,
    /// 5-bit opcode.
    pub opcode: u8,
    /// 16-bit association ID (or 0 for non-association ops).
    pub assoc_id: u16,
    /// 16-bit sequence number (must match between request/response).
    pub sequence: u16,
    /// Variable-length payload. For requests, this carries command
    /// arguments; for responses, this carries result data.
    pub data: Vec<u8>,
}

/// Pack the first byte: LI=0 (no warning) | VN=2 | Mode=6.
fn first_byte() -> u8 {
    CTRL_VN_MODE
}

/// Pack the second byte: Response(1) | More(1) | Error(1) | Opcode(5).
fn second_byte(msg: &CtrlMsg) -> u8 {
    let mut b: u8 = 0;
    if msg.response {
        b |= 0x80;
    }
    if msg.more {
        b |= 0x40;
    }
    if msg.error {
        b |= 0x20;
    }
    b |= msg.opcode & 0x1F;
    b
}

/// Parse a Mode-6 control message from the wire.
pub fn parse(input: &[u8]) -> Result<CtrlMsg, String> {
    if input.len() < CTRL_HEADER_LEN {
        return Err(format!(
            "control message too short: need {} bytes, have {}",
            CTRL_HEADER_LEN,
            input.len()
        ));
    }
    let b0 = input[0];
    let li = (b0 >> 6) & 0x03;
    let vn = (b0 >> 3) & 0x07;
    let mode = b0 & 0x07;
    if mode != 6 {
        return Err(format!("bad mode: {} (expected 6)", mode));
    }
    // NTP version 2 through 4 are acceptable for control messages.
    if !(2..=4).contains(&vn) {
        return Err(format!("bad version: {}", vn));
    }
    // LI may be 0, 1, 2, or 3; we accept any and don't echo it back.
    let _ = li;

    let b1 = input[1];
    let response = (b1 & 0x80) != 0;
    let more = (b1 & 0x40) != 0;
    let error = (b1 & 0x20) != 0;
    let opcode = b1 & 0x1F;

    let sequence = u16::from_be_bytes([input[2], input[3]]);
    let status = u16::from_be_bytes([input[4], input[5]]);
    let _ = status;
    let assoc_id = u16::from_be_bytes([input[6], input[7]]);
    let offset = u16::from_be_bytes([input[8], input[9]]);
    let _ = offset;
    let count = u32::from_be_bytes([input[10], input[11], input[12], input[13]]);
    // Reserved bytes 14..48 must be zero in valid messages; we tolerate
    // nonzero values (some clients set them) but surface as data.
    let mut data = input[CTRL_HEADER_LEN..].to_vec();
    if count as usize > data.len() {
        return Err(format!(
            "count {} exceeds payload length {}",
            count,
            data.len()
        ));
    }
    data.truncate(count as usize);

    Ok(CtrlMsg {
        response,
        error,
        more,
        opcode,
        assoc_id,
        sequence,
        data,
    })
}

/// Serialize a Mode-6 control message to the wire.
pub fn build(msg: &CtrlMsg) -> Vec<u8> {
    let mut out = Vec::with_capacity(CTRL_HEADER_LEN + msg.data.len());
    out.push(first_byte());
    out.push(second_byte(msg));
    out.extend_from_slice(&msg.sequence.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // status (MBZ on request)
    out.extend_from_slice(&msg.assoc_id.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // offset (MBZ on request)
    let count = msg.data.len() as u32;
    out.extend_from_slice(&count.to_be_bytes());
    // Reserved bytes 14..48: zero-fill.
    out.extend_from_slice(&[0u8; 34]);
    out.extend_from_slice(&msg.data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> CtrlMsg {
        CtrlMsg {
            response: false,
            error: false,
            more: false,
            opcode: 1, // READSTAT
            assoc_id: 0,
            sequence: 42,
            data: Vec::new(),
        }
    }

    fn sample_response() -> CtrlMsg {
        CtrlMsg {
            response: true,
            error: false,
            more: false,
            opcode: 1,
            assoc_id: 0,
            sequence: 42,
            data: b"assoc=1,status=8001".to_vec(),
        }
    }

    #[test]
    fn reads_request() {
        let msg = sample_request();
        let bytes = build(&msg);
        let parsed = parse(&bytes).expect("parse");
        assert!(!parsed.response);
        assert_eq!(parsed.opcode, 1);
        assert_eq!(parsed.sequence, 42);
        assert_eq!(parsed.assoc_id, 0);
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn reads_response() {
        let msg = sample_response();
        let bytes = build(&msg);
        let parsed = parse(&bytes).expect("parse");
        assert!(parsed.response);
        assert_eq!(parsed.opcode, 1);
        assert_eq!(parsed.sequence, 42);
        assert_eq!(parsed.data, b"assoc=1,status=8001");
    }

    #[test]
    fn round_trip_all_fields() {
        let msg = CtrlMsg {
            response: true,
            error: true,
            more: true,
            opcode: 31, // max 5-bit
            assoc_id: 0xDEAD,
            sequence: 0x1234,
            data: b"oops".to_vec(),
        };
        let bytes = build(&msg);
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.response, msg.response);
        assert_eq!(parsed.error, msg.error);
        assert_eq!(parsed.more, msg.more);
        assert_eq!(parsed.opcode, msg.opcode);
        assert_eq!(parsed.assoc_id, msg.assoc_id);
        assert_eq!(parsed.sequence, msg.sequence);
        assert_eq!(parsed.data, msg.data);
    }

    #[test]
    fn rejects_bad_version() {
        let mut bytes = build(&sample_request());
        // VN=3, Mode=6, LI=0: 0x18 | 0x06 = 0x1E
        bytes[0] = 0x1E; // VN=3 is OK actually; bump to VN=5
        bytes[0] = (5u8 << 3) | 6u8; // VN=5, Mode=6
        let res = parse(&bytes);
        assert!(res.is_err(), "expected bad-version error, got {:?}", res);
    }

    #[test]
    fn rejects_bad_mode() {
        let mut bytes = build(&sample_request());
        bytes[0] = (2u8 << 3) | 7u8; // VN=2, Mode=7 (private)
        let res = parse(&bytes);
        assert!(res.is_err(), "expected bad-mode error, got {:?}", res);
    }

    #[test]
    fn opcode_round_trips_through_wire() {
        // Each valid 5-bit opcode must round-trip.
        for opcode in 0u8..=31 {
            let msg = CtrlMsg {
                response: false,
                error: false,
                more: false,
                opcode,
                assoc_id: 0,
                sequence: 1,
                data: Vec::new(),
            };
            let bytes = build(&msg);
            let parsed = parse(&bytes).expect("parse");
            assert_eq!(parsed.opcode, opcode, "opcode {} failed round-trip", opcode);
        }
    }

    #[test]
    fn too_short_input_is_rejected() {
        let res = parse(&[0u8; 47]);
        assert!(res.is_err());
        let res = parse(&[]);
        assert!(res.is_err());
    }

    #[test]
    fn flags_are_packed_into_second_byte() {
        let msg = CtrlMsg {
            response: true,
            error: true,
            more: true,
            opcode: 0x1F,
            assoc_id: 0,
            sequence: 0,
            data: Vec::new(),
        };
        let bytes = build(&msg);
        // LI=0, VN=2, Mode=6 -> 0x16
        assert_eq!(bytes[0], 0x16);
        // Response(1)|More(1)|Error(1)|Opcode(0x1F) -> 0xFF
        assert_eq!(bytes[1], 0xFF);
    }

    #[test]
    fn count_field_truncates_oversized_payload() {
        // Manually craft a header whose count is smaller than the
        // payload length; parse() should truncate to count.
        let mut bytes = build(&sample_response());
        // Set count to 5 even though data has more bytes.
        bytes[10..14].copy_from_slice(&5u32.to_be_bytes());
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.data.len(), 5);
    }

    #[test]
    fn count_field_overrun_is_rejected() {
        // Set count > actual payload length; parse should refuse.
        let mut bytes = build(&sample_request());
        bytes[10..14].copy_from_slice(&9999u32.to_be_bytes());
        let res = parse(&bytes);
        assert!(res.is_err(), "expected count-overrun error");
    }

    #[test]
    fn version_three_is_accepted() {
        let mut bytes = build(&sample_request());
        bytes[0] = (3u8 << 3) | 6u8; // VN=3, Mode=6
        let parsed = parse(&bytes).expect("parse");
        assert_eq!(parsed.sequence, 42);
    }
}