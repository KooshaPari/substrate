//! Bencode — BitTorrent's serialization format (encoder + decoder).
//!
//! Bencode supports four value types:
//! - **Byte strings**: `<length>:<bytes>` (decimal length, ASCII colon, raw bytes).
//! - **Integers**: `i<signed-decimal>e`.
//! - **Lists**: `l<bencoded-values>e`.
//! - **Dictionaries**: `d<sorted-key>bencoded-value-pairs>e` with byte-string
//!   keys sorted lexicographically by raw bytes (per the BEP-3 specification).
//!
//! Reference: BEP-3, "The BitTorrent Protocol Specification"
//! (<http://www.bittorrent.org/beps/bep_0003.html>), §"Bencoding".
//!
//! Decoding is implemented as a pull-style parser over a `&[u8]` with a
//! cursor. Encoding recursively flattens a [`BencodeValue`] into a `Vec<u8>`.

use std::collections::BTreeMap;

/// A decoded bencode value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BencodeValue {
    Bytes(Vec<u8>),
    Int(i64),
    List(Vec<BencodeValue>),
    Dict(BTreeMap<Vec<u8>, BencodeValue>),
}

/// Errors produced by [`decode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Input ended before the value was complete.
    UnexpectedEof,
    /// A non-digit character appeared where a digit was expected
    /// (length prefix, integer digits, etc.).
    InvalidDigit(u8),
    /// An unknown leading byte was encountered.
    InvalidLeadingByte(u8),
    /// An integer had no terminator (eof before `e`).
    UnterminatedInteger,
    /// A list or dict had no terminator.
    UnterminatedContainer(&'static str),
    /// A dictionary contained a non-byte-string key.
    DictKeyNotBytes,
    /// A length prefix exceeded the remaining input.
    LengthExceedsInput(usize),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::UnexpectedEof => f.write_str("unexpected end of input"),
            DecodeError::InvalidDigit(b) => write!(f, "expected digit, got 0x{b:02x}"),
            DecodeError::InvalidLeadingByte(b) => write!(f, "invalid leading byte 0x{b:02x}"),
            DecodeError::UnterminatedInteger => f.write_str("unterminated integer (missing 'e')"),
            DecodeError::UnterminatedContainer(c) => {
                write!(f, "unterminated {c} (missing 'e')")
            }
            DecodeError::DictKeyNotBytes => f.write_str("dictionary key must be a byte string"),
            DecodeError::LengthExceedsInput(n) => write!(f, "length {n} exceeds input"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Encode a [`BencodeValue`] into a byte vector.
pub fn encode(value: &BencodeValue) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(value, &mut out);
    out
}

fn encode_into(value: &BencodeValue, out: &mut Vec<u8>) {
    match value {
        BencodeValue::Bytes(b) => {
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.push(b':');
            out.extend_from_slice(b);
        }
        BencodeValue::Int(i) => {
            out.push(b'i');
            out.extend_from_slice(i.to_string().as_bytes());
            out.push(b'e');
        }
        BencodeValue::List(items) => {
            out.push(b'l');
            for item in items {
                encode_into(item, out);
            }
            out.push(b'e');
        }
        BencodeValue::Dict(map) => {
            out.push(b'd');
            for (k, v) in map {
                // BEP-3: keys must be byte strings, sorted lexicographically.
                // BTreeMap with Vec<u8> keys sorts lexicographically — perfect.
                out.extend_from_slice(k.len().to_string().as_bytes());
                out.push(b':');
                out.extend_from_slice(k);
                encode_into(v, out);
            }
            out.push(b'e');
        }
    }
}

/// Decode a single bencode value from the front of `input`.
///
/// On success returns `(value, bytes_consumed)`.
pub fn decode(input: &[u8]) -> Result<(BencodeValue, usize), DecodeError> {
    let mut cur = 0usize;
    let v = decode_value(input, &mut cur)?;
    Ok((v, cur))
}

fn decode_value(input: &[u8], cur: &mut usize) -> Result<BencodeValue, DecodeError> {
    if *cur >= input.len() {
        return Err(DecodeError::UnexpectedEof);
    }
    let b = input[*cur];
    match b {
        b'i' => decode_int(input, cur),
        b'l' => decode_list(input, cur),
        b'd' => decode_dict(input, cur),
        b'0'..=b'9' => decode_bytes(input, cur),
        _ => Err(DecodeError::InvalidLeadingByte(b)),
    }
}

fn decode_int(input: &[u8], cur: &mut usize) -> Result<BencodeValue, DecodeError> {
    // Already at 'i'. Skip it.
    *cur += 1;
    let start = *cur;
    while *cur < input.len() && input[*cur] != b'e' {
        *cur += 1;
    }
    if *cur >= input.len() {
        return Err(DecodeError::UnterminatedInteger);
    }
    let s =
        std::str::from_utf8(&input[start..*cur]).map_err(|_| DecodeError::InvalidDigit(b'?'))?;
    let i: i64 = s.parse().map_err(|_| DecodeError::InvalidDigit(b'?'))?;
    *cur += 1; // consume 'e'
    Ok(BencodeValue::Int(i))
}

fn decode_bytes(input: &[u8], cur: &mut usize) -> Result<BencodeValue, DecodeError> {
    let start = *cur;
    while *cur < input.len() && input[*cur] != b':' {
        let c = input[*cur];
        if !c.is_ascii_digit() {
            return Err(DecodeError::InvalidDigit(c));
        }
        *cur += 1;
    }
    if *cur >= input.len() {
        return Err(DecodeError::UnexpectedEof);
    }
    let s =
        std::str::from_utf8(&input[start..*cur]).map_err(|_| DecodeError::InvalidDigit(b'?'))?;
    let len: usize = s.parse().map_err(|_| DecodeError::InvalidDigit(b'?'))?;
    *cur += 1; // consume ':'
    if len > input.len() - *cur {
        return Err(DecodeError::LengthExceedsInput(len));
    }
    let bytes = input[*cur..*cur + len].to_vec();
    *cur += len;
    Ok(BencodeValue::Bytes(bytes))
}

fn decode_list(input: &[u8], cur: &mut usize) -> Result<BencodeValue, DecodeError> {
    *cur += 1; // consume 'l'
    let mut out = Vec::new();
    while *cur < input.len() && input[*cur] != b'e' {
        out.push(decode_value(input, cur)?);
    }
    if *cur >= input.len() {
        return Err(DecodeError::UnterminatedContainer("list"));
    }
    *cur += 1; // consume 'e'
    Ok(BencodeValue::List(out))
}

fn decode_dict(input: &[u8], cur: &mut usize) -> Result<BencodeValue, DecodeError> {
    *cur += 1; // consume 'd'
    let mut map: BTreeMap<Vec<u8>, BencodeValue> = BTreeMap::new();
    while *cur < input.len() && input[*cur] != b'e' {
        // Key must be a byte string.
        let key = match decode_value(input, cur)? {
            BencodeValue::Bytes(k) => k,
            _ => return Err(DecodeError::DictKeyNotBytes),
        };
        let val = decode_value(input, cur)?;
        map.insert(key, val);
    }
    if *cur >= input.len() {
        return Err(DecodeError::UnterminatedContainer("dict"));
    }
    *cur += 1; // consume 'e'
    Ok(BencodeValue::Dict(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_int() {
        let v = BencodeValue::Int(42);
        let e = encode(&v);
        assert_eq!(e, b"i42e");
        let (d, n) = decode(&e).unwrap();
        assert_eq!(d, v);
        assert_eq!(n, e.len());
    }

    #[test]
    fn encode_decode_negative_int() {
        let v = BencodeValue::Int(-7);
        let e = encode(&v);
        assert_eq!(e, b"i-7e");
        let (d, _) = decode(&e).unwrap();
        assert_eq!(d, v);
    }

    #[test]
    fn encode_decode_bytes() {
        let v = BencodeValue::Bytes(b"spam".to_vec());
        let e = encode(&v);
        assert_eq!(e, b"4:spam");
        let (d, _) = decode(&e).unwrap();
        assert_eq!(d, v);
    }

    #[test]
    fn encode_decode_empty_bytes() {
        let v = BencodeValue::Bytes(Vec::new());
        let e = encode(&v);
        assert_eq!(e, b"0:");
        let (d, _) = decode(&e).unwrap();
        assert_eq!(d, v);
    }

    #[test]
    fn encode_decode_list() {
        let v = BencodeValue::List(vec![
            BencodeValue::Int(1),
            BencodeValue::Bytes(b"two".to_vec()),
            BencodeValue::Int(3),
        ]);
        let e = encode(&v);
        assert_eq!(e, b"li1e3:twoi3ee");
        let (d, _) = decode(&e).unwrap();
        assert_eq!(d, v);
    }

    #[test]
    fn encode_decode_dict_keys_sorted() {
        let mut m = BTreeMap::new();
        m.insert(b"publisher".to_vec(), BencodeValue::Bytes(b"bob".to_vec()));
        m.insert(b"artist".to_vec(), BencodeValue::Bytes(b"alice".to_vec()));
        let v = BencodeValue::Dict(m.clone());
        let e = encode(&v);
        // artist < publisher lexicographically.
        assert_eq!(e, b"d6:artist5:alice9:publisher3:bobe");
        let (d, _) = decode(&e).unwrap();
        assert_eq!(d, v);
    }

    #[test]
    fn torrent_info_example() {
        // Excerpted from BEP-3 example.
        let raw = b"d8:announce27:http://tracker.example.com/4:infod5:filesld6:lengthi1024e4:pathl10:README.txteeeee";
        let (v, n) = decode(raw).unwrap();
        assert_eq!(n, raw.len());
        if let BencodeValue::Dict(d) = v {
            assert_eq!(
                d.get(b"announce".as_ref()).unwrap(),
                &BencodeValue::Bytes(b"http://tracker.example.com/".to_vec())
            );
        } else {
            panic!("expected dict");
        }
    }

    #[test]
    fn subparse_announce() {
        let r = b"d8:announce27:http://tracker.example.com/4:infodee";
        let (v, n) = decode(r).unwrap();
        assert_eq!(n, r.len());
        if let BencodeValue::Dict(d) = v {
            let announce = d.get(b"announce".as_ref()).unwrap();
            assert_eq!(
                announce,
                &BencodeValue::Bytes(b"http://tracker.example.com/".to_vec())
            );
        } else {
            panic!();
        }
    }

    #[test]
    fn subparse_files() {
        let r = b"d5:filesld6:lengthi1024e4:pathl10:README.txteeee";
        let (v, n) = decode(r).unwrap();
        assert_eq!(n, r.len());
        if let BencodeValue::Dict(d) = v {
            let files = d.get(b"files".as_ref()).unwrap();
            assert!(matches!(files, BencodeValue::List(_)));
        } else {
            panic!();
        }
    }

    #[test]
    fn trailing_bytes_after_value() {
        let raw = b"i42ejunk";
        let (v, n) = decode(raw).unwrap();
        assert_eq!(v, BencodeValue::Int(42));
        assert_eq!(n, 4);
        assert_eq!(&raw[n..], b"junk");
    }

    #[test]
    fn empty_list_and_dict() {
        assert_eq!(encode(&BencodeValue::List(vec![])), b"le");
        assert_eq!(encode(&BencodeValue::Dict(BTreeMap::new())), b"de");
        let (l, _) = decode(b"le").unwrap();
        assert_eq!(l, BencodeValue::List(vec![]));
        let (d, _) = decode(b"de").unwrap();
        assert_eq!(d, BencodeValue::Dict(BTreeMap::new()));
    }

    #[test]
    fn invalid_leading_byte_errors() {
        assert!(matches!(
            decode(b"x1:e"),
            Err(DecodeError::InvalidLeadingByte(b'x'))
        ));
    }

    #[test]
    fn unterminated_int_errors() {
        assert!(matches!(
            decode(b"i42"),
            Err(DecodeError::UnterminatedInteger)
        ));
    }

    #[test]
    fn length_exceeds_input() {
        assert!(matches!(
            decode(b"10:short"),
            Err(DecodeError::LengthExceedsInput(10))
        ));
    }

    #[test]
    fn round_trip_nested() {
        let mut inner = BTreeMap::new();
        inner.insert(b"x".to_vec(), BencodeValue::Int(1));
        let v = BencodeValue::Dict({
            let mut m = BTreeMap::new();
            m.insert(
                b"list".to_vec(),
                BencodeValue::List(vec![BencodeValue::Dict(inner)]),
            );
            m
        });
        let e = encode(&v);
        let (d, _) = decode(&e).unwrap();
        assert_eq!(d, v);
    }
}
