// Minimal QOI (Quite OK Image) decoder.
//
// References:
//   QOI specification: https://qoiformat.org/qoi-specification.pdf
//   Header: 14 bytes
//     - magic: "qoif" (4 bytes)
//     - width: u32 big-endian (4 bytes)
//     - height: u32 big-endian (4 bytes)
//     - channels: u8 (3 = RGB, 4 = RGBA)
//     - colorspace: u8 (0 = sRGB, 1 = linear)
//   End marker: 8 zero bytes followed by a single 0x01 byte
//     (0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x01)
//   Chunks:
//     - QOI_OP_INDEX  (0b00xxxxxx): 6-bit index into running array
//     - QOI_OP_DIFF   (0b01xxxxxx): 2-bit-per-channel delta (-2..+1)
//     - QOI_OP_LUMA   (0b10xxxxxx): 8-bit green delta + 4-bit red/blue delta
//     - QOI_OP_RUN    (0b11xxxxxx with bits 5..4 != 11): run 1..62
//     - QOI_OP_RGB    (0xFE prefix): explicit RGB
//     - QOI_OP_RGBA   (0xFF prefix): explicit RGBA
//   Running state:
//     - index[64] of previously seen RGBA pixels (initialised to zero+alpha=0)
//     - previous pixel (px) starts at (0, 0, 0, 255)
//     - hash = (r*3 + g*5 + b*7 + a*11) % 64, used to pick index slot
//
// This decoder is intentionally minimal: it rejects (rather than truncates) on
// any malformed input, and only supports the 6 chunk types listed above.

const QOI_OP_RGB_TAG: u8 = 0xFE;
const QOI_OP_RGBA_TAG: u8 = 0xFF;

const QOI_END_MARKER: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];

#[derive(Debug, Clone, PartialEq)]
pub struct QoiImage {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub colorspace: u8,
    pub pixels: Vec<u8>,
}

fn hash_px(r: u8, g: u8, b: u8, a: u8) -> usize {
    ((r as usize * 3) + (g as usize * 5) + (b as usize * 7) + (a as usize * 11)) % 64
}

fn read_u32_be(input: &[u8], off: usize) -> Result<u32, String> {
    let bytes: [u8; 4] = input
        .get(off..off + 4)
        .ok_or_else(|| "qoi: truncated header".to_string())?
        .try_into()
        .map_err(|_| "qoi: truncated header".to_string())?;
    Ok(u32::from_be_bytes(bytes))
}

/// Decode a QOI image from the given bytes.
///
/// Returns an error string for malformed input (truncated header, bad magic,
/// invalid channel count, mismatched pixel buffer size, missing end marker).
pub fn decode(input: &[u8]) -> Result<QoiImage, String> {
    if input.len() < 14 {
        return Err("qoi: input shorter than 14-byte header".to_string());
    }
    if &input[0..4] != b"qoif" {
        return Err("qoi: bad magic, expected 'qoif'".to_string());
    }
    let width = read_u32_be(input, 4)?;
    let height = read_u32_be(input, 8)?;
    let channels = input[12];
    let colorspace = input[13];
    if channels != 3 && channels != 4 {
        return Err(format!("qoi: invalid channels {channels}, expected 3 or 4"));
    }
    let expected_pixels = (width as u64)
        .checked_mul(height as u64)
        .ok_or_else(|| "qoi: width*height overflow".to_string())?;
    let expected_bytes = expected_pixels
        .checked_mul(channels as u64)
        .ok_or_else(|| "qoi: pixel byte count overflow".to_string())?;
    if expected_bytes > usize::MAX as u64 {
        return Err("qoi: pixel buffer too large for this platform".to_string());
    }
    let mut index: [[u8; 4]; 64] = [[0, 0, 0, 0]; 64];
    let mut px: [u8; 4] = [0, 0, 0, 255];
    let mut out: Vec<u8> = Vec::with_capacity(expected_bytes as usize);

    let mut pos = 14usize;
    let body_end = input
        .len()
        .checked_sub(QOI_END_MARKER.len())
        .ok_or_else(|| "qoi: missing end marker".to_string())?;
    while pos < body_end {
        let b1 = input[pos];
        if b1 == QOI_OP_RGB_TAG {
            if pos + 4 > body_end {
                return Err("qoi: truncated RGB chunk".to_string());
            }
            px[0] = input[pos + 1];
            px[1] = input[pos + 2];
            px[2] = input[pos + 3];
            // px[3] unchanged
            index[hash_px(px[0], px[1], px[2], px[3])] = px;
            out.extend_from_slice(&px[..channels as usize]);
            pos += 4;
        } else if b1 == QOI_OP_RGBA_TAG {
            if pos + 5 > body_end {
                return Err("qoi: truncated RGBA chunk".to_string());
            }
            px[0] = input[pos + 1];
            px[1] = input[pos + 2];
            px[2] = input[pos + 3];
            px[3] = input[pos + 4];
            index[hash_px(px[0], px[1], px[2], px[3])] = px;
            out.extend_from_slice(&px[..channels as usize]);
            pos += 5;
        } else {
            let top2 = b1 & 0xC0;
            match top2 {
                0b00_000000 => {
                    let idx = (b1 & 0x3F) as usize;
                    px = index[idx];
                    out.extend_from_slice(&px[..channels as usize]);
                    pos += 1;
                }
                0b01_000000 => {
                    let r = (b1 >> 4) & 0x03;
                    let g = (b1 >> 2) & 0x03;
                    let bb = b1 & 0x03;
                    px[0] = px[0].wrapping_add_signed(r as i8 - 2);
                    px[1] = px[1].wrapping_add_signed(g as i8 - 2);
                    px[2] = px[2].wrapping_add_signed(bb as i8 - 2);
                    index[hash_px(px[0], px[1], px[2], px[3])] = px;
                    out.extend_from_slice(&px[..channels as usize]);
                    pos += 1;
                }
                0b10_000000 => {
                    if pos + 2 > body_end {
                        return Err("qoi: truncated LUMA chunk".to_string());
                    }
                    let b2 = input[pos + 1];
                    let vg = (b1 & 0x3F) as i8 - 32;
                    let vr = (b2 >> 4) as i8 - 8;
                    let vb = (b2 & 0x0F) as i8 - 8;
                    px[0] = px[0].wrapping_add_signed(vg + vr);
                    px[1] = px[1].wrapping_add_signed(vg);
                    px[2] = px[2].wrapping_add_signed(vg + vb);
                    index[hash_px(px[0], px[1], px[2], px[3])] = px;
                    out.extend_from_slice(&px[..channels as usize]);
                    pos += 2;
                }
                0b11_000000 => {
                    // RUN chunk: bits 5..4 must NOT be 11 (those are RGB/RGBA,
                    // already handled above). Valid range: 0xC0..0xFD.
                    let run_len = (b1 & 0x3F) as usize + 1;
                    if run_len < 1 || run_len > 62 {
                        return Err(format!("qoi: invalid run length {run_len}"));
                    }
                    for _ in 0..run_len {
                        out.extend_from_slice(&px[..channels as usize]);
                    }
                    pos += 1;
                }
                _ => unreachable!(),
            }
        }
    }

    // Verify end marker.
    if input.len() < QOI_END_MARKER.len() {
        return Err("qoi: missing end marker".to_string());
    }
    let tail_start = input.len() - QOI_END_MARKER.len();
    if input[tail_start..] != QOI_END_MARKER {
        return Err("qoi: end marker mismatch".to_string());
    }

    if out.len() as u64 != expected_bytes {
        return Err(format!(
            "qoi: decoded {} bytes, expected {} (width={} height={} channels={})",
            out.len(),
            expected_bytes,
            width,
            height,
            channels
        ));
    }

    Ok(QoiImage {
        width,
        height,
        channels,
        colorspace,
        pixels: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(width: u32, height: u32, channels: u8, colorspace: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"qoif");
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.push(channels);
        v.push(colorspace);
        v.extend_from_slice(body);
        v.extend_from_slice(&QOI_END_MARKER);
        v
    }

    #[test]
    fn rejects_short_input() {
        assert!(decode(&[0, 1, 2]).is_err());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut v = build(1, 1, 4, 0, &[]);
        v[0] = b'X';
        assert!(decode(&v).is_err());
    }

    #[test]
    fn rejects_bad_channels() {
        let v = build(1, 1, 5, 0, &[]);
        assert!(decode(&v).is_err());
    }

    #[test]
    fn rejects_missing_end_marker() {
        // Build a header with no body and NO end marker (truncate after header).
        let mut v = Vec::new();
        v.extend_from_slice(b"qoif");
        v.extend_from_slice(&1u32.to_be_bytes());
        v.extend_from_slice(&1u32.to_be_bytes());
        v.push(4);
        v.push(0);
        let err = decode(&v).unwrap_err();
        assert!(err.contains("end marker"));
    }

    #[test]
    fn rejects_truncated_end_marker() {
        let mut v = build(1, 1, 4, 0, &[]);
        // Strip the end marker entirely
        v.truncate(14);
        assert!(decode(&v).is_err());
    }

    #[test]
    fn decodes_single_op_rgb() {
        // 1x1 RGBA image with one RGB chunk (alpha stays at initial 255)
        let body = vec![0xFE, 10, 20, 30];
        let buf = build(1, 1, 4, 0, &body);
        let img = decode(&buf).expect("decode");
        assert_eq!(img.width, 1);
        assert_eq!(img.height, 1);
        assert_eq!(img.channels, 4);
        assert_eq!(img.colorspace, 0);
        assert_eq!(img.pixels, vec![10, 20, 30, 255]);
    }

    #[test]
    fn decodes_op_rgba() {
        // 1x1 RGBA, single RGBA chunk
        let body = vec![0xFF, 1, 2, 3, 4];
        let buf = build(1, 1, 4, 0, &body);
        let img = decode(&buf).expect("decode");
        assert_eq!(img.pixels, vec![1, 2, 3, 4]);
    }

    #[test]
    fn decodes_op_index() {
        // Emit OP_RGB (sets px, leaves alpha at 255), then 3 INDEX ops.
        // Hash(10,20,30,255) = (30+100+210+2805) % 64 = 3145 % 64 = 9
        let idx = hash_px(10, 20, 30, 255) as u8;
        let mut body = vec![0xFE, 10, 20, 30];
        body.extend_from_slice(&[idx & 0x3F; 3]);
        let buf = build(1, 4, 4, 0, &body); // 4 pixels = 1 RGB + 3 INDEX
        let img = decode(&buf).expect("decode");
        assert_eq!(img.pixels.len(), 4 * 4);
        for px in img.pixels.chunks_exact(4) {
            assert_eq!(px, &[10, 20, 30, 255]);
        }
    }

    #[test]
    fn decodes_op_diff() {
        // DIFF chunk: dr/dg/db = (bits) - 2, applied to current px.
        // 0x40 in binary is 01000000 -> r=0,g=0,b=0 -> deltas -2,-2,-2.
        // Starting from (0,0,0,255), repeated 4 times yields a 2-decrement
        // ramp: (254), (252), (250), (248). Assert that progression.
        let body = vec![0x40, 0x40, 0x40, 0x40];
        let buf = build(1, 4, 4, 0, &body);
        let img = decode(&buf).expect("decode");
        let expected = [254u8, 252, 250, 248];
        for (i, px) in img.pixels.chunks_exact(4).enumerate() {
            assert_eq!(px, &[expected[i], expected[i], expected[i], 255]);
        }
    }

    #[test]
    fn decodes_op_luma() {
        // LUMA chunk: 2 bytes.
        //   byte1 top 2 bits = 10, low 6 bits encode vg as (b1 & 0x3F) - 32.
        //   byte2 encodes (vr<<4 | vb) where each 4-bit field is biased by -8.
        // To get vg=0, vr=0, vb=0: byte1 = 0x80 | 32 = 0xA0, byte2 = (8<<4)|8 = 0x88.
        let body = vec![0xA0, 0x88, 0xA0, 0x88];
        let buf = build(1, 2, 4, 0, &body);
        let img = decode(&buf).expect("decode");
        for px in img.pixels.chunks_exact(4) {
            assert_eq!(px, &[0, 0, 0, 255]);
        }
    }

    #[test]
    fn decodes_op_run() {
        // First emit one RGB to set px, then a 1-pixel RUN chunk repeated.
        // 0xC0 -> top2 = 11, bits 5..4 = 00 (valid RUN), run_len = 0+1 = 1.
        let mut body = vec![0xFE, 50, 60, 70];
        body.extend_from_slice(&[0xC0; 3]); // 3 more runs of length 1
        let buf = build(1, 4, 4, 0, &body);
        let img = decode(&buf).expect("decode");
        for px in img.pixels.chunks_exact(4) {
            assert_eq!(px, &[50, 60, 70, 255]);
        }
    }

    #[test]
    fn decodes_op_run_long() {
        // Single RUN chunk with run_len = 62 (max).
        // 0xC0 | 61 = 0xFD = 0b11_111101. run_len = (0xFD & 0x3F) + 1 = 61 + 1 = 62.
        // Plus one RGB to set px, then 62-pixel RUN.
        let mut body = vec![0xFE, 1, 2, 3, 0xFD];
        // Now we need 1 + 62 = 63 pixels; declare a 63x1 image.
        let buf = build(63, 1, 4, 0, &body);
        let img = decode(&buf).expect("decode");
        assert_eq!(img.pixels.len(), 63 * 4);
        for px in img.pixels.chunks_exact(4) {
            assert_eq!(px, &[1, 2, 3, 255]);
        }
    }

    #[test]
    fn hash_matches_qoi_spec_formula() {
        // Reference implementation hash: (r*3 + g*5 + b*7 + a*11) % 64.
        // Cross-checked against the qoiformat.org C reference.
        assert_eq!(hash_px(0, 0, 0, 255), (255 * 11) % 64);
        assert_eq!(hash_px(255, 0, 0, 255), (255 * 3 + 255 * 11) % 64);
        assert_eq!(hash_px(0, 255, 0, 255), (255 * 5 + 255 * 11) % 64);
        assert_eq!(hash_px(0, 0, 255, 255), (255 * 7 + 255 * 11) % 64);
        assert_eq!(hash_px(255, 255, 255, 255), 255 * (3 + 5 + 7 + 11) % 64);
        // Concrete values:
        // (0,0,0,255)   = 2805 % 64 = 53
        // (255,0,0,255) = (765 + 2805) % 64 = 3570 % 64 = 50
        assert_eq!(hash_px(0, 0, 0, 255), 53);
        assert_eq!(hash_px(255, 0, 0, 255), 50);
    }

    #[test]
    fn decodes_official_8x8_red_header_bytes() {
        // QOI official test vector: 8x8 red image with channels=3 colorspace=1
        // has header bytes: 71 6f 69 66 00 00 00 08 00 00 00 08 00 01
        let v = build(8, 8, 3, 1, &[]);
        assert_eq!(&v[0..4], b"qoif");
        assert_eq!(u32::from_be_bytes([v[4], v[5], v[6], v[7]]), 8);
        assert_eq!(u32::from_be_bytes([v[8], v[9], v[10], v[11]]), 8);
        assert_eq!(v[12], 3); // channels
        assert_eq!(v[13], 1); // colorspace
                              // Bare header (no body, no end marker beyond what's appended) should
                              // error because the body cannot yield 8*8*3 = 192 bytes.
        assert!(decode(&v).is_err());
    }
}
