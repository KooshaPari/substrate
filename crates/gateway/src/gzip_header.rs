pub const MAGIC: [u8; 2] = [0x1f, 0x8b];

pub struct GzipHeader {
    pub mtime: u32,
    pub xfl: u8,
    pub os: u8,
    pub extra: Option<Vec<u8>>,
    pub name: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug)]
pub enum GzipParseError {
    TooShort,
    BadMagic,
    UnknownCompression,
}

pub fn parse_header(data: &[u8]) -> Result<(GzipHeader, usize), GzipParseError> {
    if data.len() < 10 {
        return Err(GzipParseError::TooShort);
    }
    if data[0] != MAGIC[0] || data[1] != MAGIC[1] {
        return Err(GzipParseError::BadMagic);
    }
    let cm = data[2];
    if cm != 8 {
        return Err(GzipParseError::UnknownCompression);
    }
    let flg = data[3];
    let mtime = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let xfl = data[8];
    let os = data[9];
    let mut pos = 10;
    let extra = if flg & 0x04 != 0 {
        if pos + 2 > data.len() {
            return Err(GzipParseError::TooShort);
        }
        let xlen = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if pos + xlen > data.len() {
            return Err(GzipParseError::TooShort);
        }
        let e = data[pos..pos + xlen].to_vec();
        pos += xlen;
        Some(e)
    } else {
        None
    };
    let name = if flg & 0x08 != 0 {
        let start = pos;
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        if pos >= data.len() {
            return Err(GzipParseError::TooShort);
        }
        let s = std::str::from_utf8(&data[start..pos])
            .map_err(|_| GzipParseError::TooShort)?
            .to_string();
        pos += 1;
        Some(s)
    } else {
        None
    };
    let comment = if flg & 0x10 != 0 {
        let start = pos;
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        if pos >= data.len() {
            return Err(GzipParseError::TooShort);
        }
        let s = std::str::from_utf8(&data[start..pos])
            .map_err(|_| GzipParseError::TooShort)?
            .to_string();
        pos += 1;
        Some(s)
    } else {
        None
    };
    Ok((
        GzipHeader {
            mtime,
            xfl,
            os,
            extra,
            name,
            comment,
        },
        pos,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hdr(cm: u8, flg: u8) -> Vec<u8> {
        let mut v = vec![0x1f, 0x8b, cm, flg, 0, 0, 0, 0, 0, 255];
        v
    }
    #[test]
    fn valid_minimal() {
        let v = hdr(8, 0);
        let (h, n) = parse_header(&v).unwrap();
        assert_eq!(h.mtime, 0);
        assert_eq!(h.xfl, 0);
        assert_eq!(h.os, 255);
        assert!(h.extra.is_none());
        assert_eq!(n, 10);
    }
    #[test]
    fn bad_magic() {
        let v = [0u8; 10];
        assert!(matches!(parse_header(&v), Err(GzipParseError::BadMagic)));
    }
    #[test]
    fn too_short() {
        let v = [0x1f, 0x8b];
        assert!(matches!(parse_header(&v), Err(GzipParseError::TooShort)));
    }
    #[test]
    fn unknown_compression() {
        let v = [0x1f, 0x8b, 9, 0, 0, 0, 0, 0, 0, 0];
        assert!(matches!(
            parse_header(&v),
            Err(GzipParseError::UnknownCompression)
        ));
    }
    #[test]
    fn with_name() {
        let mut v = hdr(8, 0x08);
        v.extend_from_slice(b"hi.txt\0");
        let (h, n) = parse_header(&v).unwrap();
        assert_eq!(h.name.as_deref(), Some("hi.txt"));
        assert_eq!(n, 17);
    }
    #[test]
    fn with_comment() {
        let mut v = hdr(8, 0x10);
        v.extend_from_slice(b"a comment\0");
        let (h, _) = parse_header(&v).unwrap();
        assert_eq!(h.comment.as_deref(), Some("a comment"));
    }
}
