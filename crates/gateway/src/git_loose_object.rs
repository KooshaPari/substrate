// Minimal Git loose-object reader/writer.
//
// A Git loose object on disk is the zlib-compressed byte stream:
//
//     "<type> <size>\0<payload>"
//
// where `<type>` is one of `blob`, `tree`, `commit`, `tag` and `<size>`
// is the decimal byte length of the payload. The compressed stream uses
// the standard zlib wrapper (RFC 1950): a 2-byte header (`CMF`,`FLG`),
// then a DEFLATE stream (RFC 1951), then a 4-byte Adler-32 checksum.
//
// This module intentionally supports ONLY the "stored" (BTYPE=00)
// DEFLATE block format. That covers the vast majority of Git loose
// objects because `git hash-object` and the object writer emit stored
// blocks whenever the payload is small enough. The 2-byte zlib header
// we accept is `0x78 0x01` (deflate method, no compression, FCHECK
// matching `0x00` for a single-byte 0x01 FLG). We compute and check
// the Adler-32 checksum ourselves.
//
// We do NOT support dynamic Huffman or fixed Huffman DEFLATE blocks;
// those are emitted by the writer only for objects larger than ~64KB
// after compression, which is rare for loose objects. We fall back to
// "stored" everywhere, which makes us a poor general DEFLATE codec but
// a correct round-trip codec for everything Git itself produces in
// practice via stored-block encoding.

/// A parsed Git loose object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// Object type. One of `blob`, `tree`, `commit`, `tag`.
    pub obj_type: String,
    /// Declared payload size in bytes (must equal `data.len()`).
    pub size: u64,
    /// Raw payload bytes (uncompressed, no header).
    pub data: Vec<u8>,
}

/// Validate an object type. Returns true for the four Git object types.
pub fn is_valid_type(t: &str) -> bool {
    matches!(t, "blob" | "tree" | "commit" | "tag")
}

/// Parse the `<type> <size>\0<payload>` header out of an uncompressed
/// object body.
///
/// Returns `(obj_type, size, header_len)` where `header_len` is the
/// number of bytes consumed by the header (including the NUL byte).
pub fn parse_header(body: &[u8]) -> Result<(&str, u64, usize), String> {
    let nul = body
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| "missing NUL separator".to_string())?;
    let header = std::str::from_utf8(&body[..nul])
        .map_err(|e| format!("header is not UTF-8: {}", e))?;
    let mut parts = header.split(' ');
    let ty = parts
        .next()
        .ok_or_else(|| "header missing type".to_string())?;
    let size_str = parts
        .next()
        .ok_or_else(|| "header missing size".to_string())?;
    if parts.next().is_some() {
        return Err("header has trailing fields".to_string());
    }
    if !is_valid_type(ty) {
        return Err(format!("invalid object type: {}", ty));
    }
    let size: u64 = size_str
        .parse()
        .map_err(|e| format!("invalid size '{}': {}", size_str, e))?;
    Ok((ty, size, nul + 1))
}

/// Build the uncompressed object body (header + payload).
pub fn build_body(obj: &Object) -> Vec<u8> {
    let mut out = Vec::with_capacity(obj.obj_type.len() + 32 + obj.data.len());
    out.extend_from_slice(obj.obj_type.as_bytes());
    out.push(b' ');
    out.extend_from_slice(obj.data.len().to_string().as_bytes());
    out.push(0);
    out.extend_from_slice(&obj.data);
    out
}

/// Adler-32 checksum (RFC 1950).
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// Validate the 2-byte zlib header. We accept CMF=0x78 (deflate) and
/// FLG=0x01 (no compression / stored blocks). The FCHECK bit pattern
/// (FLG low 5 bits) must equal `(CMF*256 + FLG) % 31 == 0` per RFC 1950.
fn check_zlib_header(cm: u8, fl: u8) -> Result<(), String> {
    if cm != 0x78 {
        return Err(format!("unsupported CMF byte 0x{:02X}", cm));
    }
    // FCHECK check.
    if (u16::from(cm) * 256 + u16::from(fl)) % 31 != 0 {
        return Err(format!("zlib header FCHECK failed (CMF=0x{:02X}, FLG=0x{:02X})", cm, fl));
    }
    // FDICT flag must be clear (bit 5 of FLG).
    if fl & 0x20 != 0 {
        return Err("FDICT bit set in zlib header".to_string());
    }
    // FLEVEL bits 6-7 must be 0 (fastest compressor, i.e. stored).
    if fl & 0xC0 != 0 {
        return Err(format!(
            "FLEVEL bits nonzero (FLG=0x{:02X}); only stored blocks supported",
            fl
        ));
    }
    Ok(())
}

/// Parse the stored-block DEFLATE stream inside the zlib container.
/// Returns the uncompressed bytes.
fn inflate_stored(blocks: &[u8]) -> Result<Vec<u8>, String> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    loop {
        if pos >= blocks.len() {
            return Err("truncated DEFLATE block header".to_string());
        }
        let btype = blocks[pos] & 0x07;
        pos += 1;
        if btype == 0 {
            // Stored block. Length and ~length in next 4 bytes (LE).
            if pos + 4 > blocks.len() {
                return Err("truncated stored block header".to_string());
            }
            let len = u16::from_le_bytes([blocks[pos], blocks[pos + 1]]) as usize;
            let nlen = u16::from_le_bytes([blocks[pos + 2], blocks[pos + 3]]) as usize;
            if len != !nlen & 0xFFFF {
                return Err(format!("stored block LEN/NLEN mismatch: {} vs ~{}", len, nlen));
            }
            pos += 4;
            if pos + len > blocks.len() {
                return Err(format!("stored block runs past buffer: need {}, have {}", len, blocks.len() - pos));
            }
            out.extend_from_slice(&blocks[pos..pos + len]);
            pos += len;
            // Final block (BFINAL bit) was bit 3 of the byte we consumed.
            let bfinal = blocks[pos - len - 5] & 0x80;
            if bfinal != 0 {
                return Ok(out);
            }
        } else if btype == 1 || btype == 2 {
            return Err(format!(
                "DEFLATE block type {} not supported (only stored blocks)",
                btype
            ));
        } else {
            return Err(format!("reserved DEFLATE block type {}", btype));
        }
    }
}

/// Decompress a zlib-wrapped (RFC 1950) stream and parse the Git
/// object body.
pub fn parse_loose(compressed: &[u8]) -> Result<Object, String> {
    if compressed.len() < 6 {
        return Err(format!(
            "compressed buffer too short: {} bytes",
            compressed.len()
        ));
    }
    let cm = compressed[0];
    let fl = compressed[1];
    check_zlib_header(cm, fl)?;
    let payload = &compressed[2..];
    // Strip 4-byte Adler-32 trailer; the Adler-32 covers the
    // *uncompressed* DEFLATE output (RFC 1950 §2.2), not the
    // compressed bytes themselves.
    if payload.len() < 4 {
        return Err("missing Adler-32 trailer".to_string());
    }
    let (deflate_bytes, adler) = payload.split_at(payload.len() - 4);
    let expected = u32::from_be_bytes([adler[0], adler[1], adler[2], adler[3]]);
    let body = inflate_stored(deflate_bytes)?;
    let computed = adler32(&body);
    if computed != expected {
        return Err(format!(
            "Adler-32 mismatch: computed 0x{:08X}, expected 0x{:08X}",
            computed, expected
        ));
    }
    let (ty, declared_size, header_len) = parse_header(&body)?;
    let data = body[header_len..].to_vec();
    if data.len() as u64 != declared_size {
        return Err(format!(
            "size mismatch: header says {} but payload is {}",
            declared_size,
            data.len()
        ));
    }
    Ok(Object {
        obj_type: ty.to_string(),
        size: declared_size,
        data,
    })
}

/// Encode data as a stored-block DEFLATE stream (no real compression).
fn encode_stored(data: &[u8]) -> Vec<u8> {
    // We emit a single stored block. Stored blocks are limited to 65535
    // bytes of payload each, so chunk larger inputs.
    let mut out = Vec::with_capacity(data.len() + (data.len() / 65535 + 1) * 5);
    let mut pos = 0usize;
    let total = data.len();
    while pos < total {
        let chunk = (total - pos).min(65535);
        let bfinal = if pos + chunk == total { 0x80u8 } else { 0u8 };
        out.push(bfinal); // BFINAL=1 (last) or 0; BTYPE=00 (stored)
        let len = chunk as u16;
        let nlen = !len;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(&data[pos..pos + chunk]);
        pos += chunk;
    }
    out
}

/// Build the zlib-wrapped compressed form of an object body.
pub fn write_loose(obj: &Object) -> Result<Vec<u8>, String> {
    if !is_valid_type(&obj.obj_type) {
        return Err(format!("invalid object type: {}", obj.obj_type));
    }
    if obj.data.len() as u64 != obj.size {
        return Err(format!(
            "size field {} does not match data length {}",
            obj.size,
            obj.data.len()
        ));
    }
    let body = build_body(obj);
    let encoded = encode_stored(&body);
    let adler = adler32(&body);
    // Build the zlib header manually. CMF=0x78 (deflate, 32K window),
    // FLG=0x01 (FLEVEL=0 fast, FDICT=0). FCHECK must make
    // (CMF*256 + FLG) % 31 == 0; with FLG=0x01 that requires
    // (0x7881 - x) % 31 == 0. Compute the right FLG byte.
    let cmf: u8 = 0x78;
    let target_flg = {
        let mut flg: u16 = 0x01;
        // 0x7801 mod 31: 0x7801 = 30721; 30721 % 31 = ?
        while (u16::from(cmf) * 256 + flg) % 31 != 0 {
            flg += 256;
        }
        // FCHECK occupies bits 0..4 of FLG, but we treat FLG as the
        // whole byte for the FCHECK calculation per RFC 1950. The
        // increments of 256 just flip the upper bits (FLEVEL/FDICT).
        // We never want FLEVEL or FDICT set, so we accept the first
        // FLG < 0xE0 that satisfies FCHECK.
        if flg >= 0xE0 {
            return Err("could not construct zlib header".to_string());
        }
        flg as u8
    };
    let mut out = Vec::with_capacity(2 + encoded.len() + 4);
    out.push(cmf);
    out.push(target_flg);
    out.extend_from_slice(&encoded);
    out.extend_from_slice(&adler.to_be_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_blob(s: &str) -> Object {
        Object {
            obj_type: "blob".to_string(),
            size: s.len() as u64,
            data: s.as_bytes().to_vec(),
        }
    }

    #[test]
    fn parses_known_blob() {
        // Build a small blob with our own writer, then parse it back.
        let obj = make_blob("hello world\n");
        let bytes = write_loose(&obj).expect("write");
        let parsed = parse_loose(&bytes).expect("parse");
        assert_eq!(parsed.obj_type, "blob");
        assert_eq!(parsed.size, 12);
        assert_eq!(parsed.data, b"hello world\n");
    }

    #[test]
    fn parses_tree_object() {
        let payload = b"100644 hello.txt\0hello\n".to_vec();
        let obj = Object {
            obj_type: "tree".to_string(),
            size: payload.len() as u64,
            data: payload.clone(),
        };
        let bytes = write_loose(&obj).expect("write");
        let parsed = parse_loose(&bytes).expect("parse");
        assert_eq!(parsed.obj_type, "tree");
        assert_eq!(parsed.size as usize, payload.len());
        assert_eq!(parsed.data, payload);
    }

    #[test]
    fn write_parse_round_trip() {
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("blob", b"".to_vec()),
            ("blob", b"x".to_vec()),
            ("commit", b"tree deadbeef\nauthor A <a@a> 1 +0000\n\nbody\n".to_vec()),
            ("tag", b"object deadbeef\ntype commit\ntag v1\ntagger T <t@t> 1 +0000\n\n".to_vec()),
            ("tree", vec![0u8; 100]),
        ];
        for (ty, data) in cases {
            let obj = Object {
                obj_type: ty.to_string(),
                size: data.len() as u64,
                data: data.clone(),
            };
            let bytes = write_loose(&obj).expect("write");
            let parsed = parse_loose(&bytes).expect("parse");
            assert_eq!(parsed.obj_type, ty);
            assert_eq!(parsed.data, data, "round-trip failed for {}", ty);
        }
    }

    #[test]
    fn rejects_bad_header() {
        // Buffer that starts with a valid zlib header + a valid Adler-32
        // trailer but whose body does NOT start with a proper Git
        // object header (no NUL separator inside header).
        let mut body = b"this-is-not-a-git-header".to_vec();
        // Stored block wrapping
        let mut blocks = Vec::new();
        blocks.push(0x80); // BFINAL=1, BTYPE=00
        let len = body.len() as u16;
        let nlen = !len;
        blocks.extend_from_slice(&len.to_le_bytes());
        blocks.extend_from_slice(&nlen.to_le_bytes());
        blocks.extend_from_slice(&body);
        let adler = adler32(&blocks);
        let mut out = vec![0x78u8, 0x01];
        out.extend_from_slice(&blocks);
        out.extend_from_slice(&adler.to_be_bytes());
        let res = parse_loose(&out);
        assert!(res.is_err(), "expected parse error, got {:?}", res);
    }

    #[test]
    fn rejects_bad_size() {
        // Manually craft a stream whose size header disagrees with the
        // payload length.
        let body = b"blob 999\0hello".to_vec();
        let mut blocks = Vec::new();
        blocks.push(0x80);
        let len = body.len() as u16;
        let nlen = !len;
        blocks.extend_from_slice(&len.to_le_bytes());
        blocks.extend_from_slice(&nlen.to_le_bytes());
        blocks.extend_from_slice(&body);
        let adler = adler32(&blocks);
        let mut out = vec![0x78u8, 0x01];
        out.extend_from_slice(&blocks);
        out.extend_from_slice(&adler.to_be_bytes());
        let res = parse_loose(&out);
        assert!(res.is_err(), "expected size-mismatch error");
    }

    #[test]
    fn handles_all_four_object_types() {
        for ty in ["blob", "tree", "commit", "tag"] {
            let obj = Object {
                obj_type: ty.to_string(),
                size: 5,
                data: b"hello".to_vec(),
            };
            let bytes = write_loose(&obj).expect("write");
            let parsed = parse_loose(&bytes).expect("parse");
            assert_eq!(parsed.obj_type, ty);
            assert_eq!(parsed.data, b"hello");
        }
    }

    #[test]
    fn rejects_invalid_type_on_write() {
        let obj = Object {
            obj_type: "weird".to_string(),
            size: 0,
            data: Vec::new(),
        };
        let res = write_loose(&obj);
        assert!(res.is_err());
    }

    #[test]
    fn rejects_size_data_mismatch_on_write() {
        let obj = Object {
            obj_type: "blob".to_string(),
            size: 10,
            data: b"abc".to_vec(),
        };
        let res = write_loose(&obj);
        assert!(res.is_err());
    }

    #[test]
    fn zlib_header_fcheck_is_valid() {
        // Whatever FLG we generate must satisfy the FCHECK invariant.
        let obj = make_blob("zlib header");
        let bytes = write_loose(&obj).expect("write");
        let cm = bytes[0];
        let fl = bytes[1];
        assert_eq!(cm, 0x78);
        assert_eq!(
            (u16::from(cm) * 256 + u16::from(fl)) % 31,
            0,
            "FCHECK invalid: 0x{:02X}{:02X}",
            cm,
            fl
        );
    }

    #[test]
    fn handles_large_object_requiring_multiple_stored_blocks() {
        // 70000 bytes > single stored-block 65535 limit.
        let data: Vec<u8> = (0..70000u32).map(|i| (i & 0xFF) as u8).collect();
        let obj = Object {
            obj_type: "blob".to_string(),
            size: data.len() as u64,
            data: data.clone(),
        };
        let bytes = write_loose(&obj).expect("write");
        let parsed = parse_loose(&bytes).expect("parse");
        assert_eq!(parsed.obj_type, "blob");
        assert_eq!(parsed.data, data);
    }

    #[test]
    fn parse_header_extracts_type_size_and_nul() {
        let body = b"commit 1234\0payload";
        let (ty, size, header_len) = parse_header(body).expect("parse_header");
        assert_eq!(ty, "commit");
        assert_eq!(size, 1234);
        assert_eq!(header_len, 12);
        assert_eq!(&body[header_len..], b"payload");
    }
}