//! Brontide replay capture format.
//!
//! Minimal reader/writer for the single-direction TCP capture files produced
//! by [Brontide](https://github.com/soundcloud/brontide), SoundCloud's tool
//! for recording and replaying TCP traffic. The format is intentionally
//! simple so it can be produced or consumed without pulling in a full
//! pcap dependency.
//!
//! # File layout
//!
//! ```text
//! +-------------------+
//! | Header (16 bytes) |
//! +-------------------+
//! | Record 0          |
//! | Record 1          |
//! | ...               |
//! +-------------------+
//! ```
//!
//! ## Header
//!
//! | Offset | Size | Field           | Notes                                |
//! |--------|------|-----------------|--------------------------------------|
//! | 0      | 4    | magic           | ASCII `"BTCD"` (Brontide CaPture Dir) |
//! | 4      | 2    | version         | Little-endian `u16`. Only `1` known. |
//! | 6      | 2    | flags           | Reserved; must be `0` on write.      |
//! | 8      | 8    | reserved        | Zero-filled. Reserved for future use.|
//!
//! ## Record
//!
//! | Offset | Size          | Field        |
//! |--------|---------------|--------------|
//! | 0      | 8             | `timestamp_us` (LE `u64`) |
//! | 2      | 2             | `src_port` (LE `u16`)     |
//! | 4      | 2             | `dst_port` (LE `u16`)     |
//! | 6      | 4             | `payload_len` (LE `u32`)  |
//! | 10     | `payload_len` | `payload` (raw bytes)     |
//!
//! `timestamp_us` is the capture time in microseconds since an unspecified
//! epoch; Brontide uses it only as a relative ordering signal. `src_port`
//! and `dst_port` carry the TCP port pair observed at capture time.
//!
//! # Errors
//!
//! [`parse`] returns a `String` describing the first malformed byte
//! sequence it encounters. [`write`] returns a `String` only if the
//! underlying `Vec<u8>` allocation fails (which on stable Rust is not
//! possible from safe code, but the signature leaves room for future
//! streaming writers).

/// Magic bytes that must appear at offset 0 of every Brontide capture file.
///
/// `"BTCD"` stands for "Brontide CaPture Directory", the SoundCloud
/// project's internal name for this format.
pub const MAGIC: &[u8; 4] = b"BTCD";

/// The only file-format version this module knows how to read or write.
pub const SUPPORTED_VERSION: u16 = 1;

/// Reserved flags word; must be zero in every file produced by [`write`].
pub const RESERVED_FLAGS: u16 = 0;

/// Total size, in bytes, of the fixed header at the start of a capture.
pub const HEADER_LEN: usize = 16;

/// A single captured packet record.
///
/// `timestamp_us` is microseconds since an epoch chosen by the capture
/// process; consumers should treat it as monotonic within a single capture
/// file. `src_port` and `dst_port` are the TCP ports observed on the
/// captured segment. `payload` holds the bytes of the TCP payload (the
/// data after the TCP header); it may be empty for segments that carry no
/// payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Microsecond timestamp recorded at capture time.
    pub timestamp_us: u64,
    /// Source TCP port of the captured segment.
    pub src_port: u16,
    /// Destination TCP port of the captured segment.
    pub dst_port: u16,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
}

/// Parse a Brontide capture blob into its records.
///
/// `input` should be the raw bytes of a `.btcd` (or whatever extension the
/// caller chooses) file starting at offset 0. The function validates the
/// magic and version fields and then walks every record, returning the full
/// vector on success or a human-readable error string on the first
/// malformed structure it hits.
///
/// Records whose `payload_len` does not match the remaining bytes in the
/// file are rejected. Records whose `payload_len` would exceed the
/// remaining bytes are rejected. Empty files (header-only) are valid and
/// return an empty vector.
pub fn parse(input: &[u8]) -> Result<Vec<Record>, String> {
    if input.len() < HEADER_LEN {
        return Err(format!(
            "brontide_replay: input shorter than header ({} < {})",
            input.len(),
            HEADER_LEN
        ));
    }

    // Magic.
    if &input[0..4] != MAGIC {
        return Err(format!(
            "brontide_replay: bad magic: expected {:?}, got {:?}",
            std::str::from_utf8(MAGIC).unwrap_or("????"),
            String::from_utf8_lossy(&input[0..4])
        ));
    }

    // Version (LE u16) and flags (LE u16).
    let version = u16::from_le_bytes([input[4], input[5]]);
    if version != SUPPORTED_VERSION {
        return Err(format!(
            "brontide_replay: unsupported version {} (only {} known)",
            version, SUPPORTED_VERSION
        ));
    }
    let flags = u16::from_le_bytes([input[6], input[7]]);
    if flags != RESERVED_FLAGS {
        return Err(format!(
            "brontide_replay: unknown flags {:#06x} (must be 0)",
            flags
        ));
    }
    // Reserved 8 bytes (input[8..16]) are not yet interpreted.

    let mut records = Vec::new();
    let mut cursor = HEADER_LEN;
    while cursor < input.len() {
        // Record fixed part is 16 bytes: u64 + u16 + u16 + u32.
        if cursor + 16 > input.len() {
            return Err(format!(
                "brontide_replay: truncated record header at offset {}",
                cursor
            ));
        }
        let timestamp_us = u64::from_le_bytes([
            input[cursor],
            input[cursor + 1],
            input[cursor + 2],
            input[cursor + 3],
            input[cursor + 4],
            input[cursor + 5],
            input[cursor + 6],
            input[cursor + 7],
        ]);
        let src_port = u16::from_le_bytes([input[cursor + 8], input[cursor + 9]]);
        let dst_port = u16::from_le_bytes([input[cursor + 10], input[cursor + 11]]);
        let payload_len = u32::from_le_bytes([
            input[cursor + 12],
            input[cursor + 13],
            input[cursor + 14],
            input[cursor + 15],
        ]) as usize;

        let payload_start = cursor + 16;
        let payload_end = match payload_start.checked_add(payload_len) {
            Some(e) => e,
            None => {
                return Err(format!(
                    "brontide_replay: payload_len {} overflows at offset {}",
                    payload_len, cursor
                ));
            }
        };
        if payload_end > input.len() {
            return Err(format!(
                "brontide_replay: payload_len {} exceeds file at offset {}",
                payload_len, cursor
            ));
        }
        let payload = input[payload_start..payload_end].to_vec();

        records.push(Record {
            timestamp_us,
            src_port,
            dst_port,
            payload,
        });
        cursor = payload_end;
    }

    Ok(records)
}

/// Serialize a list of records into a Brontide capture blob.
///
/// The output always starts with a valid 16-byte header (`MAGIC`,
/// `SUPPORTED_VERSION`, zero flags, zero reserved) followed by the records
/// in input order. The current implementation never fails for in-memory
/// inputs; the `Result` return type exists so that future streaming
/// writers can report I/O or allocation errors without breaking callers.
pub fn write(records: &[Record]) -> Result<Vec<u8>, String> {
    // Pre-size the buffer to avoid reallocations on large captures.
    let mut out: Vec<u8> = Vec::with_capacity(HEADER_LEN + records.len() * 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&RESERVED_FLAGS.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]); // reserved

    for r in records {
        out.extend_from_slice(&r.timestamp_us.to_le_bytes());
        out.extend_from_slice(&r.src_port.to_le_bytes());
        out.extend_from_slice(&r.dst_port.to_le_bytes());
        let payload_len: u32 = r.payload.len() as u32;
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&r.payload);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a single-record capture blob with arbitrary fields.
    fn build_one(ts: u64, src: u16, dst: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
        v.extend_from_slice(&RESERVED_FLAGS.to_le_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v.extend_from_slice(&ts.to_le_bytes());
        v.extend_from_slice(&src.to_le_bytes());
        v.extend_from_slice(&dst.to_le_bytes());
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn parse_known_capture() {
        let blob = build_one(1_234_567, 44321, 80, b"GET / HTTP/1.0\r\n\r\n");
        let recs = parse(&blob).expect("parse should succeed");
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].timestamp_us, 1_234_567);
        assert_eq!(recs[0].src_port, 44321);
        assert_eq!(recs[0].dst_port, 80);
        assert_eq!(recs[0].payload, b"GET / HTTP/1.0\r\n\r\n");
    }

    #[test]
    fn round_trip_preserves_records() {
        let original = vec![
            Record {
                timestamp_us: 0,
                src_port: 1,
                dst_port: 2,
                payload: vec![],
            },
            Record {
                timestamp_us: u64::MAX,
                src_port: 65535,
                dst_port: 0,
                payload: vec![0xde, 0xad, 0xbe, 0xef],
            },
            Record {
                timestamp_us: 42,
                src_port: 22,
                dst_port: 54321,
                payload: b"hello".to_vec(),
            },
        ];
        let bytes = write(&original).expect("write should succeed");
        let parsed = parse(&bytes).expect("parse should succeed");
        assert_eq!(parsed, original);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut blob = build_one(0, 1, 2, b"x");
        blob[0] = b'X';
        let err = parse(&blob).expect_err("bad magic should fail");
        assert!(err.contains("bad magic"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut blob = build_one(0, 1, 2, b"x");
        // Bump version to 99 at offset 4..6.
        blob[4] = 99;
        blob[5] = 0;
        let err = parse(&blob).expect_err("unknown version should fail");
        assert!(
            err.contains("unsupported version"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn empty_capture_is_valid() {
        let bytes = write(&[]).expect("empty write should succeed");
        assert_eq!(bytes.len(), HEADER_LEN);
        let parsed = parse(&bytes).expect("empty parse should succeed");
        assert!(parsed.is_empty());
    }

    #[test]
    fn large_payload_round_trip() {
        let payload: Vec<u8> = (0..4096).map(|i| (i & 0xff) as u8).collect();
        let records = vec![Record {
            timestamp_us: 999_999_999,
            src_port: 31337,
            dst_port: 8080,
            payload: payload.clone(),
        }];
        let bytes = write(&records).expect("write large should succeed");
        // Header (16) + record header (16) + payload (4096).
        assert_eq!(bytes.len(), HEADER_LEN + 16 + payload.len());
        let parsed = parse(&bytes).expect("parse large should succeed");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].payload, payload);
    }

    #[test]
    fn rejects_truncated_payload() {
        // Build a record that claims a 10-byte payload but only ship 5 bytes.
        let mut blob = Vec::new();
        blob.extend_from_slice(MAGIC);
        blob.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
        blob.extend_from_slice(&RESERVED_FLAGS.to_le_bytes());
        blob.extend_from_slice(&[0u8; 8]);
        blob.extend_from_slice(&0u64.to_le_bytes());
        blob.extend_from_slice(&1u16.to_le_bytes());
        blob.extend_from_slice(&2u16.to_le_bytes());
        blob.extend_from_slice(&10u32.to_le_bytes()); // claims 10 bytes
        blob.extend_from_slice(&[1u8, 2, 3, 4, 5]); // only 5
        let err = parse(&blob).expect_err("truncated payload should fail");
        assert!(
            err.contains("exceeds file") || err.contains("truncated"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn rejects_input_shorter_than_header() {
        let err = parse(&[0u8; 4]).expect_err("short input should fail");
        assert!(
            err.contains("shorter than header"),
            "unexpected error: {}",
            err
        );
    }
}
