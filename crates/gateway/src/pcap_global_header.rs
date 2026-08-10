// Minimal libpcap file format reader (https://wiki.wireshark.org/Development/LibpcapFileFormat).
// Parses the 24-byte global header (magic + version + thiszone + sigfigs +
// snaplen + network) and the per-packet record header (ts_sec, ts_usec,
// incl_len, orig_len). Does NOT decode packet payloads.
//
// Two magic values are supported:
//   - 0xa1b2c3d4 — microsecond timestamps (canonical)
//   - 0xa1b23c4d — nanosecond timestamps (modified)

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PcapGlobalHeader {
    pub magic: u32,
    pub version_major: u16,
    pub version_minor: u16,
    pub thiszone: i32,
    pub sigfigs: u32,
    pub snaplen: u32,
    pub network: u32,
    pub nanosecond_timestamps: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PcapRecordHeader {
    pub ts_sec: u32,
    pub ts_usec_or_nsec: u32,
    pub incl_len: u32,
    pub orig_len: u32,
    pub payload: Vec<u8>,
}

pub fn parse_global_header(input: &[u8]) -> Result<PcapGlobalHeader, String> {
    if input.len() < 24 {
        return Err("pcap header truncated".into());
    }
    let magic_le = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
    let magic_be = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let (magic, swapped) = match (magic_le, magic_be) {
        (0xa1b2c3d4, _) => (0xa1b2c3d4, false),
        (0xa1b23c4d, _) => (0xa1b23c4d, false),
        (_, 0xa1b2c3d4) => (0xa1b2c3d4, true),
        (_, 0xa1b23c4d) => (0xa1b23c4d, true),
        _ => return Err(format!("not a pcap file (magic=0x{:08x})", magic_le)),
    };
    let nano = magic == 0xa1b23c4d;
    let read_u16 = |off: usize| -> u16 {
        if swapped {
            u16::from_be_bytes([input[off], input[off + 1]])
        } else {
            u16::from_le_bytes([input[off], input[off + 1]])
        }
    };
    let read_u32 = |off: usize| -> u32 {
        if swapped {
            u32::from_be_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        } else {
            u32::from_le_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        }
    };
    let read_i32 = |off: usize| -> i32 {
        if swapped {
            i32::from_be_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        } else {
            i32::from_le_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        }
    };
    Ok(PcapGlobalHeader {
        magic,
        nanosecond_timestamps: nano,
        version_major: read_u16(4),
        version_minor: read_u16(6),
        thiszone: read_i32(8),
        sigfigs: read_u32(12),
        snaplen: read_u32(16),
        network: read_u32(20),
    })
}

pub fn parse_record<'a>(
    input: &'a [u8],
    header: &PcapGlobalHeader,
) -> Result<(PcapRecordHeader, &'a [u8]), String> {
    if input.len() < 16 {
        return Err("record header truncated".into());
    }
    let swapped = header.magic == u32::from_be_bytes(header.magic.to_le_bytes())
        && header.magic != 0xa1b2c3d4
        && header.magic != 0xa1b23c4d;
    let read_u32 = |off: usize| -> u32 {
        if swapped {
            u32::from_be_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        } else {
            u32::from_le_bytes([input[off], input[off + 1], input[off + 2], input[off + 3]])
        }
    };
    let ts_sec = read_u32(0);
    let ts_usec = read_u32(4);
    let incl_len = read_u32(8);
    let orig_len = read_u32(12);
    if input.len() < 16 + incl_len as usize {
        return Err("record payload truncated".into());
    }
    let payload = input[16..16 + incl_len as usize].to_vec();
    let rest = &input[16 + incl_len as usize..];
    Ok((
        PcapRecordHeader {
            ts_sec,
            ts_usec_or_nsec: ts_usec,
            incl_len,
            orig_len,
            payload,
        },
        rest,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mk_pcap_le(magic: u32, network: u32, snaplen: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&magic.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&4u16.to_le_bytes());
        v.extend_from_slice(&0i32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&snaplen.to_le_bytes());
        v.extend_from_slice(&network.to_le_bytes());
        v
    }
    #[test]
    fn parse_le_micro() {
        let buf = mk_pcap_le(0xa1b2c3d4, 1, 65535);
        let h = parse_global_header(&buf).unwrap();
        assert_eq!(h.magic, 0xa1b2c3d4);
        assert!(!h.nanosecond_timestamps);
        assert_eq!(h.network, 1);
        assert_eq!(h.snaplen, 65535);
        assert_eq!(h.version_major, 2);
    }
    #[test]
    fn parse_le_nano() {
        let buf = mk_pcap_le(0xa1b23c4d, 101, 262144);
        let h = parse_global_header(&buf).unwrap();
        assert!(h.nanosecond_timestamps);
        assert_eq!(h.network, 101);
    }
    #[test]
    fn parse_be_micro() {
        // swapped-endian header
        let mut buf = Vec::new();
        buf.extend_from_slice(&0xa1b2c3d4u32.to_be_bytes());
        buf.extend_from_slice(&2u16.to_be_bytes());
        buf.extend_from_slice(&4u16.to_be_bytes());
        buf.extend_from_slice(&(-1i32).to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&65535u32.to_be_bytes());
        buf.extend_from_slice(&113u32.to_be_bytes());
        let h = parse_global_header(&buf).unwrap();
        assert_eq!(h.network, 113);
        assert_eq!(h.thiszone, -1);
    }
    #[test]
    fn truncated() {
        assert!(parse_global_header(&[0u8; 10]).is_err());
    }
    #[test]
    fn bad_magic() {
        let buf = mk_pcap_le(0xdeadbeef, 1, 65535);
        assert!(parse_global_header(&buf).is_err());
    }
    #[test]
    fn parse_record_basic() {
        let buf = mk_pcap_le(0xa1b2c3d4, 1, 65535);
        let h = parse_global_header(&buf).unwrap();
        let mut rec = Vec::new();
        rec.extend_from_slice(&1700000000u32.to_le_bytes()); // ts_sec
        rec.extend_from_slice(&123456u32.to_le_bytes()); // ts_usec
        rec.extend_from_slice(&5u32.to_le_bytes()); // incl_len
        rec.extend_from_slice(&5u32.to_le_bytes()); // orig_len
        rec.extend_from_slice(b"hello");
        let (rh, rest) = parse_record(&rec, &h).unwrap();
        assert_eq!(rh.ts_sec, 1700000000);
        assert_eq!(rh.ts_usec_or_nsec, 123456);
        assert_eq!(rh.incl_len, 5);
        assert_eq!(rh.payload, b"hello");
        assert!(rest.is_empty());
    }
    #[test]
    fn parse_record_truncated_payload() {
        let buf = mk_pcap_le(0xa1b2c3d4, 1, 65535);
        let h = parse_global_header(&buf).unwrap();
        let mut rec = Vec::new();
        rec.extend_from_slice(&1u32.to_le_bytes());
        rec.extend_from_slice(&2u32.to_le_bytes());
        rec.extend_from_slice(&10u32.to_le_bytes()); // incl_len=10
        rec.extend_from_slice(&10u32.to_le_bytes());
        rec.extend_from_slice(b"short");
        assert!(parse_record(&rec, &h).is_err());
    }
    #[test]
    fn parse_multiple_records() {
        let buf = mk_pcap_le(0xa1b2c3d4, 1, 65535);
        let h = parse_global_header(&buf).unwrap();
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(b"abc");
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(&5u32.to_le_bytes());
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(b"wxyz");
        let (r1, rest) = parse_record(&data, &h).unwrap();
        assert_eq!(r1.payload, b"abc");
        let (r2, rest) = parse_record(rest, &h).unwrap();
        assert_eq!(r2.payload, b"wxyz");
        assert!(rest.is_empty());
    }
}
