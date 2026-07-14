// Minimal BMP (Windows Bitmap) parser.
//
// References:
//   BITMAPFILEHEADER (14 bytes, all little-endian):
//     - 0x00: signature "BM" (2 bytes)
//     - 0x02: file size in bytes (4 bytes)
//     - 0x06: reserved (2 bytes, app-specific)
//     - 0x08: reserved (2 bytes, app-specific)
//     - 0x0A: offset to pixel array (4 bytes)
//   BITMAPINFOHEADER (40 bytes, the standard Windows variant):
//     - 0x0E: header size = 40 (4 bytes)
//     - 0x12: width (i32, signed; negative => top-down)
//     - 0x16: height (i32, signed; negative => top-down)
//     - 0x1A: planes (u16, must be 1)
//     - 0x1C: bits per pixel (u16)
//     - 0x1E: compression method (u32, BI_RGB = 0)
//     - 0x22: raw image size (u32, may be 0 for BI_RGB)
//     - 0x26: horizontal resolution (ppm)
//     - 0x2A: vertical resolution (ppm)
//     - 0x2E: colors in palette (0 = default 2^n)
//     - 0x32: important colors (0 = all)
//
//   Each row is padded to a multiple of 4 bytes:
//     RowSize = floor((bpp * width + 31) / 32) * 4
//
//   Pixel data layout (BI_RGB):
//     - 24bpp: B, G, R per pixel (no alpha)
//     - 32bpp: B, G, R, X per pixel (X = padding/alpha per BI_BITFIELDS)
//
// This parser is intentionally minimal: it supports 24bpp and 32bpp
// uncompressed BMPs (BI_RGB) only. All values are read as little-endian.

const BMP_MAGIC: [u8; 2] = [b'B', b'M'];
const BMP_INFO_HEADER_SIZE: u32 = 40;
const BI_RGB: u32 = 0;

#[derive(Debug, Clone, PartialEq)]
pub struct Bmp {
    pub width: i32,
    pub height: i32,
    pub bits_per_pixel: u16,
    pub compression: u32,
    pub pixel_data: Vec<u8>,
    pub bytes_per_row: usize,
}

fn read_u16_le(input: &[u8], off: usize) -> Result<u16, String> {
    let bytes: [u8; 2] = input
        .get(off..off + 2)
        .ok_or_else(|| "bmp: truncated u16".to_string())?
        .try_into()
        .map_err(|_| "bmp: truncated u16".to_string())?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32_le(input: &[u8], off: usize) -> Result<u32, String> {
    let bytes: [u8; 4] = input
        .get(off..off + 4)
        .ok_or_else(|| "bmp: truncated u32".to_string())?
        .try_into()
        .map_err(|_| "bmp: truncated u32".to_string())?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_i32_le(input: &[u8], off: usize) -> Result<i32, String> {
    Ok(read_u32_le(input, off)? as i32)
}

/// Parse a BMP file body. Supports 24bpp and 32bpp BI_RGB only.
pub fn parse(input: &[u8]) -> Result<Bmp, String> {
    if input.len() < 14 {
        return Err("bmp: input shorter than 14-byte file header".to_string());
    }
    if input[0..2] != BMP_MAGIC {
        return Err("bmp: bad magic, expected 'BM'".to_string());
    }
    let _file_size = read_u32_le(input, 2)?;
    let _reserved1 = read_u16_le(input, 6)?;
    let _reserved2 = read_u16_le(input, 8)?;
    let pixel_offset = read_u32_le(input, 10)? as usize;

    // Read BITMAPINFOHEADER (must be 40-byte variant for this minimal parser).
    let header_size = read_u32_le(input, 14)?;
    if header_size < BMP_INFO_HEADER_SIZE {
        return Err(format!(
            "bmp: header size {header_size} smaller than BITMAPINFOHEADER (40)"
        ));
    }
    if header_size != BMP_INFO_HEADER_SIZE {
        return Err(format!(
            "bmp: only BITMAPINFOHEADER (40 bytes) supported, got {header_size}"
        ));
    }
    let width = read_i32_le(input, 18)?;
    let height = read_i32_le(input, 22)?;
    let planes = read_u16_le(input, 26)?;
    if planes != 1 {
        return Err(format!("bmp: invalid planes {planes}, expected 1"));
    }
    let bits_per_pixel = read_u16_le(input, 28)?;
    if bits_per_pixel != 24 && bits_per_pixel != 32 {
        return Err(format!(
            "bmp: only 24bpp and 32bpp supported, got {bits_per_pixel}"
        ));
    }
    let compression = read_u32_le(input, 30)?;
    if compression != BI_RGB {
        return Err(format!(
            "bmp: only BI_RGB (0) compression supported, got {compression}"
        ));
    }
    let _raw_size = read_u32_le(input, 34)?;
    let _xres = read_u32_le(input, 38)?;
    let _yres = read_u32_le(input, 42)?;
    let _colors = read_u32_le(input, 46)?;
    let _important = read_u32_le(input, 50)?;

    if width == 0 {
        return Err("bmp: width is zero".to_string());
    }
    if width.unsigned_abs() as u64 > u32::MAX as u64 {
        return Err("bmp: width too large".to_string());
    }
    // height can be 0 or negative for top-down; reject only 0 here.
    if height == 0 {
        return Err("bmp: height is zero".to_string());
    }

    let abs_height = height.unsigned_abs() as u64;
    let abs_width = width.unsigned_abs() as u64;
    let bpp = bits_per_pixel as u64;

    // bytes_per_row = floor((bpp * width + 31) / 32) * 4
    let bytes_per_row_bits = bpp
        .checked_mul(abs_width)
        .ok_or_else(|| "bmp: bpp*width overflow".to_string())?;
    let bytes_per_row = (bytes_per_row_bits
        .checked_add(31)
        .ok_or_else(|| "bmp: row size overflow".to_string())?
        / 32)
        * 4;
    if bytes_per_row > usize::MAX as u64 {
        return Err("bmp: row size exceeds platform usize".to_string());
    }
    let bytes_per_row = bytes_per_row as usize;

    let total_bytes = (bytes_per_row as u64)
        .checked_mul(abs_height)
        .ok_or_else(|| "bmp: total pixel size overflow".to_string())?;
    let end = (pixel_offset as u64)
        .checked_add(total_bytes)
        .ok_or_else(|| "bmp: pixel end overflow".to_string())?;
    if end > input.len() as u64 {
        return Err(format!(
            "bmp: pixel data extends past end of input ({} > {})",
            end,
            input.len()
        ));
    }

    let pixel_data = input[pixel_offset..pixel_offset + total_bytes as usize].to_vec();

    Ok(Bmp {
        width,
        height,
        bits_per_pixel,
        compression,
        pixel_data,
        bytes_per_row,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_bmp(width: i32, height: i32, bpp: u16, pixels: &[u8]) -> Vec<u8> {
        // Row padding to 4-byte alignment
        let abs_w = width.unsigned_abs() as usize;
        let abs_h = height.unsigned_abs() as usize;
        let bytes_per_row_data = (bpp as usize / 8) * abs_w;
        let row_padded = (bytes_per_row_data + 3) & !3;
        assert_eq!(
            pixels.len(),
            row_padded * abs_h,
            "test fixture pixel data must match padded row size"
        );
        let file_size = 14 + 40 + pixels.len();
        let mut v = Vec::new();
        v.extend_from_slice(b"BM");
        v.extend_from_slice(&(file_size as u32).to_le_bytes());
        v.extend_from_slice(&[0u8; 4]); // reserved1 + reserved2
        v.extend_from_slice(&(14u32 + 40).to_le_bytes()); // pixel offset
                                                          // BITMAPINFOHEADER
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&width.to_le_bytes());
        v.extend_from_slice(&height.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // planes
        v.extend_from_slice(&bpp.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        v.extend_from_slice(&0u32.to_le_bytes()); // raw size
        v.extend_from_slice(&0u32.to_le_bytes()); // xres
        v.extend_from_slice(&0u32.to_le_bytes()); // yres
        v.extend_from_slice(&0u32.to_le_bytes()); // colors
        v.extend_from_slice(&0u32.to_le_bytes()); // important
        v.extend_from_slice(pixels);
        v
    }

    #[test]
    fn rejects_short_input() {
        assert!(parse(&[0, 1, 2]).is_err());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut v = vec![b'X', b'Y'];
        v.extend_from_slice(&[0; 52]);
        assert!(parse(&v).is_err());
    }

    #[test]
    fn rejects_zero_width() {
        let mut v = Vec::new();
        v.extend_from_slice(b"BM");
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&(14u32 + 40).to_le_bytes());
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&0i32.to_le_bytes()); // width = 0
        v.extend_from_slice(&1i32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&24u16.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&[0; 16]);
        assert!(parse(&v).is_err());
    }

    #[test]
    fn rejects_unsupported_bpp() {
        let pixels = vec![0u8; 4];
        let v = build_bmp(2, 1, 16, &pixels);
        let err = parse(&v).unwrap_err();
        assert!(err.contains("16bpp") || err.contains("only"));
    }

    #[test]
    fn rejects_unsupported_compression() {
        // Manually build a 24bpp BMP but with compression = 1 (BI_RLE8).
        let mut v = Vec::new();
        v.extend_from_slice(b"BM");
        v.extend_from_slice(&100u32.to_le_bytes());
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&(14u32 + 40).to_le_bytes());
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&2i32.to_le_bytes());
        v.extend_from_slice(&1i32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&24u16.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes()); // BI_RLE8
        v.extend_from_slice(&[0; 16]);
        assert!(parse(&v).is_err());
    }

    #[test]
    fn parses_24bpp_simple() {
        // 2x2 24bpp: each row is 2*3 = 6 bytes data + 2 bytes padding = 8 bytes.
        // Total = 2 rows * 8 = 16 bytes.
        let pixels = vec![
            // Row 0: blue, green
            255, 0, 0, 0, 255, 0, 0, 0, // trailing 2 bytes padding
            // Row 1: red, white
            0, 0, 255, 255, 255, 255, 0, 0,
        ];
        let v = build_bmp(2, 2, 24, &pixels);
        let bmp = parse(&v).expect("parse");
        assert_eq!(bmp.width, 2);
        assert_eq!(bmp.height, 2);
        assert_eq!(bmp.bits_per_pixel, 24);
        assert_eq!(bmp.compression, 0);
        assert_eq!(bmp.bytes_per_row, 8);
        assert_eq!(bmp.pixel_data.len(), 16);
        // Row 0 pixel bytes preserved
        assert_eq!(&bmp.pixel_data[0..6], &[255, 0, 0, 0, 255, 0]);
        // Row 1 pixel bytes preserved
        assert_eq!(&bmp.pixel_data[8..14], &[0, 0, 255, 255, 255, 255]);
    }

    #[test]
    fn parses_24bpp_with_row_padding() {
        // 1x3 24bpp: row = 3 bytes + 1 byte padding = 4 bytes; total = 12 bytes
        let pixels = vec![
            1, 2, 3, 0, // pixel + padding
            4, 5, 6, 0, 7, 8, 9, 0,
        ];
        let v = build_bmp(1, 3, 24, &pixels);
        let bmp = parse(&v).expect("parse");
        assert_eq!(bmp.bytes_per_row, 4);
        assert_eq!(bmp.pixel_data.len(), 12);
        // Cross-check formula: floor((24*1 + 31)/32)*4 = floor(55/32)*4 = 1*4 = 4 ✓
    }

    #[test]
    fn parses_32bpp() {
        // 2x1 32bpp = 8 bytes (no row padding required at multiples of 4)
        let pixels = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let v = build_bmp(2, 1, 32, &pixels);
        let bmp = parse(&v).expect("parse");
        assert_eq!(bmp.bits_per_pixel, 32);
        assert_eq!(bmp.bytes_per_row, 8);
        assert_eq!(bmp.pixel_data, pixels);
    }

    #[test]
    fn parses_negative_height_top_down() {
        // Negative height means top-down row order. width=2 24bpp.
        // Each row = 6 bytes data + 2 padding = 8 bytes. Total = 16 bytes.
        let pixels = vec![
            // Row 0
            1, 2, 3, 4, 5, 6, 0, 0, // Row 1
            7, 8, 9, 10, 11, 12, 0, 0,
        ];
        let v = build_bmp(2, -2, 24, &pixels);
        let bmp = parse(&v).expect("parse");
        assert_eq!(bmp.width, 2);
        assert_eq!(bmp.height, -2);
        assert_eq!(bmp.bytes_per_row, 8);
        assert_eq!(bmp.pixel_data.len(), 16);
    }

    #[test]
    fn rejects_truncated_pixel_data() {
        // Build a full 2x2 BMP then chop off the last byte of pixel data.
        let pixels = vec![255; 16]; // 2 rows of 8 padded bytes
        let mut v = build_bmp(2, 2, 24, &pixels);
        v.pop();
        assert!(parse(&v).is_err());
    }
}
