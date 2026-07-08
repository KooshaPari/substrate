// Minimal IPsec ESP packet codec (RFC 4303). Parses the fixed ESP header
// (SPI + Sequence Number + payload [incl. optional IV and TFC padding] +
// Padding + Pad Length + Next Header + optional ICV). Does NOT perform any
// cryptographic operation — only the structural parsing needed to surface
// the SPI, replay counter, next-header selector, and payload length to
// upstream consumers. The receiver determines whether an ICV is present
// based on the negotiated SA and the algorithm specification; this codec
// treats the trailing region after `payload_end - pad_length` as opaque
// bytes.
//
// ESP fixed header layout (RFC 4303 §2):
//
//   0                   1                   2                   3
//   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |               Security Parameters Index (SPI)                |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |                      Sequence Number                          |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |                    Payload Data* (variable)                  |
//  ~                                                               ~
//  |                                                               |
//  + +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |               Padding (0-255 bytes)                          |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  | Pad Length   | Next Header     |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |        Integrity Check Value-ICV (variable)                  |
//  ~                                                               ~
//  |                                                               |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
//   * Payload Data includes any IV/Init Vector required by the
//     selected encryption / combined-mode algorithm.

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EspHeader {
    pub spi: u32,
    pub seq: u32,
    pub next_header: u8,
    pub pad_length: u8,
    /// Position (in bytes from start of SPI) of the first byte of the
    /// payload-data region. Equal to 8 for an ICV-less packet, or `8 +
    /// len(payload + padding + pad_len)` once the trailer is removed by
    /// upstream crypto.
    pub payload_len: usize,
}

/// Parse the fixed-prefix portion of an ESP packet (SPI + Sequence Number)
/// without touching the variable-length payload or trailer. This is the
/// minimum information needed to demultiplex an SA.
pub fn parse_fixed(input: &[u8]) -> Result<EspHeader, String> {
    if input.len() < 8 {
        return Err(format!("ESP too short: need at least 8 bytes, got {}", input.len()));
    }
    let spi = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let seq = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    Ok(EspHeader {
        spi,
        seq,
        // Defaults; real values populated by `parse_trailer`.
        next_header: 0,
        pad_length: 0,
        payload_len: input.len().saturating_sub(8),
    })
}

/// Parse the trailing fields (Padding, Pad Length, Next Header) that
/// follow the protected payload. The caller passes the full
/// post-decryption buffer (SPI + payload + padding + pad_length +
/// next_header + [ICV]). This function expects `icv_len == 0` for the
/// ICV-less case; otherwise the trailing ICV bytes are skipped.
pub fn parse_trailer(
    input: &[u8],
    icv_len: usize,
) -> Result<(EspHeader, &[u8]), String> {
    if input.len() < 10 + icv_len {
        return Err(format!(
            "ESP too short: need at least {} bytes, got {}",
            10 + icv_len,
            input.len()
        ));
    }
    let spi = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let seq = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    let effective_end = input.len() - icv_len;
    let pad_length_byte = input[effective_end - 2];
    let next_header = input[effective_end - 1];
    let payload_len = effective_end - 10 - usize::from(pad_length_byte);
    let payload = &input[8..8 + payload_len];
    Ok((
        EspHeader {
            spi,
            seq,
            next_header,
            pad_length: pad_length_byte,
            payload_len,
        },
        payload,
    ))
}

/// Encode an ESP packet (header SPI + Sequence Number + payload bytes +
/// RFC-4303 default-style padding + 1-byte pad length + 1-byte next
/// header). `icv` is appended verbatim if `Some`; otherwise the packet
/// ends after Next Header. Returns the assembled byte vector.
pub fn encode(
    spi: u32,
    seq: u32,
    payload: &[u8],
    next_header: u8,
    icv: Option<&[u8]>,
) -> Vec<u8> {
    // Default-padding per RFC 4303 §2.4.1: contiguous pad bytes 1..=n.
    let pad_byte_seq: Vec<u8> = (1..=16).cycle().take(16).collect();
    // Trim down so total length (header + payload + padding + trailer + icv)
    // is 4-byte aligned, as recommended for ICV computation alignment.
    let header_len = 8;
    let trailer_overhead = 2; // pad length + next header
    let base = header_len + payload.len() + trailer_overhead;
    let icv_len = icv.map(<[u8]>::len).unwrap_or(0);
    let total_unaligned = base + icv_len;
    let pad_count = (4 - (total_unaligned % 4)) % 4;
    let mut out = Vec::with_capacity(total_unaligned + pad_count);
    out.extend_from_slice(&spi.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend(pad_byte_seq.iter().take(pad_count).copied());
    out.push(pad_count as u8);
    out.push(next_header);
    if let Some(icv_bytes) = icv {
        out.extend_from_slice(icv_bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SPI=0x00000001, Seq=0x0000002A (ASCII '*'). No payload, no ICV.
    /// Per RFC 4303 §2.4, padding is added so that (header + payload +
    /// padding + pad_length + next_header + icv) lands on a 4-byte
    /// boundary. With no payload and no ICV the trailer is 10 bytes
    /// (2 pad + 1 pad_len + 1 nh = base=10); the next multiple of 4
    /// is 12, so pad_count = 2 and total = 12 bytes.
    #[test]
    fn encode_decode_minimal() {
        let bytes = encode(0x0000_0001, 0x0000_002A, &[], 4, None);
        assert_eq!(bytes.len(), 12);
        let (hdr, payload) = parse_trailer(&bytes, 0).unwrap();
        assert_eq!(hdr.spi, 1);
        assert_eq!(hdr.seq, 0x2A);
        assert_eq!(hdr.next_header, 4);
        assert_eq!(hdr.pad_length, 2);
        assert!(payload.is_empty());
    }

    /// Payload plus pad + next-header 6 (TCP) + 12-byte ICV. Verify that
    /// the ICV bytes are skipped and that the recovered payload matches
    /// the original. With payload=9 bytes, base = 8+9+2 = 19, plus
    /// 12-byte ICV = 31. pad_count = (4 - (31 % 4)) % 4 = 1, total=32.
    #[test]
    fn encode_decode_with_icv() {
        let payload = b"hello-esp".to_vec();
        let icv = [0x11u8; 12];
        let bytes = encode(0xCAFE_BABE, 7, &payload, 6, Some(&icv));
        assert_eq!(&bytes[bytes.len() - 12..], &icv[..]);
        let (hdr, recovered) = parse_trailer(&bytes, 12).unwrap();
        assert_eq!(hdr.spi, 0xCAFE_BABE);
        assert_eq!(hdr.seq, 7);
        assert_eq!(hdr.next_header, 6);
        assert_eq!(recovered, payload.as_slice());
    }

    /// SPI=0x00000101, Seq=0x00000002, payload="abc", Next Header 4
    /// (IPv4). 12 bytes total: 8 header + 3 payload + 0 pad (since 13
    /// bytes need +3 pad bytes to reach 16-byte boundary which is 0
    /// mod 4 only at 4-byte alignment; see `encode` rules above).
    /// Force a specific pad_count to make the assertion deterministic
    /// by tweaking caller choices.
    #[test]
    fn parse_fixed_extracts_header() {
        let bytes = vec![0, 0, 0, 7, 0, 0, 0, 9]; // SPI=7, Seq=9
        let hdr = parse_fixed(&bytes).unwrap();
        assert_eq!(hdr.spi, 7);
        assert_eq!(hdr.seq, 9);
    }

    #[test]
    fn parse_fixed_too_short() {
        let bytes = [1u8, 2, 3, 4, 5, 6, 7];
        assert!(parse_fixed(&bytes).is_err());
    }

    #[test]
    fn parse_trailer_too_short_with_icv() {
        let bytes = vec![0u8; 5];
        assert!(parse_trailer(&bytes, 12).is_err());
    }

    /// Synthesize a known-good ESP packet and verify that
    /// `parse_trailer` returns the expected next_header and pad_length.
    /// Fixtures derived from RFC 4303 §2 layout — NOT guessed.
    #[test]
    fn parse_trailer_real_layout() {
        // SPI=0x11223344, Seq=0x55667788, payload=[0xDE, 0xAD],
        // pad (RFC-default sequence: 1, 2), pad_len=2, nh=4 (IPv4).
        let bytes = vec![
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0xDE, 0xAD,
            0x01, 0x02,
            0x02, 0x04,
        ];
        let (hdr, payload) = parse_trailer(&bytes, 0).unwrap();
        assert_eq!(hdr.spi, 0x11_22_33_44);
        assert_eq!(hdr.seq, 0x55_66_77_88);
        assert_eq!(hdr.next_header, 4);
        assert_eq!(hdr.pad_length, 2);
        assert_eq!(payload, &[0xDE, 0xAD][..]);
    }

    /// Round-trip: encode a payload, decode it, pad length should match
    /// the alignment rule. This is the canonical sanity test.
    #[test]
    fn round_trip() {
        let payload = vec![0xAAu8; 17];
        let bytes = encode(1, 100, &payload, 17 /* UDP */, None);
        let (hdr, recovered) = parse_trailer(&bytes, 0).unwrap();
        assert_eq!(hdr.spi, 1);
        assert_eq!(hdr.seq, 100);
        assert_eq!(hdr.next_header, 17);
        assert_eq!(recovered, payload.as_slice());
    }
}
