// Minimal Redis RESP2 protocol encoder/decoder
#[derive(Debug, PartialEq, Clone)]
pub enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<RespValue>>),
}

pub fn encode(v: &RespValue) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(v, &mut out);
    out
}
fn encode_into(v: &RespValue, out: &mut Vec<u8>) {
    match v {
        RespValue::SimpleString(s) => { out.push(b'+'); out.extend_from_slice(s.as_bytes()); out.extend_from_slice(b"\r\n"); }
        RespValue::Error(s) => { out.push(b'-'); out.extend_from_slice(s.as_bytes()); out.extend_from_slice(b"\r\n"); }
        RespValue::Integer(n) => { out.push(b':'); out.extend_from_slice(n.to_string().as_bytes()); out.extend_from_slice(b"\r\n"); }
        RespValue::BulkString(Some(b)) => {
            out.push(b'$');
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        RespValue::BulkString(None) => { out.extend_from_slice(b"$-1\r\n"); }
        RespValue::Array(Some(items)) => {
            out.push(b'*');
            out.extend_from_slice(items.len().to_string().as_bytes());
            out.extend_from_slice(b"\r\n");
            for item in items { encode_into(item, out); }
        }
        RespValue::Array(None) => { out.extend_from_slice(b"*-1\r\n"); }
    }
}
#[derive(Debug, PartialEq)]
pub enum ParseError {
    Empty,
    BadPrefix,
    BadInteger,
    Incomplete,
    BadLength,
}
pub fn parse(input: &[u8]) -> Result<(RespValue, usize), ParseError> {
    if input.is_empty() { return Err(ParseError::Empty); }
    parse_value(input, 0).ok_or(ParseError::Incomplete)
}
fn parse_value(input: &[u8], pos: usize) -> Option<(RespValue, usize)> {
    if pos >= input.len() { return None; }
    match input[pos] {
        b'+' => read_line(input, pos + 1).map(|(s, p)| (RespValue::SimpleString(s), p)),
        b'-' => read_line(input, pos + 1).map(|(s, p)| (RespValue::Error(s), p)),
        b':' => read_line(input, pos + 1)
            .and_then(|(s, p)| s.parse::<i64>().ok().map(|n| (RespValue::Integer(n), p))),
        b'$' => {
            let (n, p) = read_line(input, pos + 1)
                .and_then(|(s, p)| s.parse::<i64>().ok().map(|n| (n, p)))?;
            if n < 0 { return Some((RespValue::BulkString(None), p)); }
            let len = n as usize;
            if p + len + 2 > input.len() { return None; }
            if &input[p + len..p + len + 2] != b"\r\n" { return None; }
            let data = input[p..p + len].to_vec();
            Some((RespValue::BulkString(Some(data)), p + len + 2))
        }
        b'*' => {
            let (n, p) = read_line(input, pos + 1)
                .and_then(|(s, p)| s.parse::<i64>().ok().map(|n| (n, p)))?;
            if n < 0 { return Some((RespValue::Array(None), p)); }
            let len = n as usize;
            let mut items = Vec::with_capacity(len);
            let mut cur = p;
            for _ in 0..len {
                let (item, next) = parse_value(input, cur)?;
                items.push(item);
                cur = next;
            }
            Some((RespValue::Array(Some(items)), cur))
        }
        _ => None,
    }
}
fn read_line(input: &[u8], pos: usize) -> Option<(String, usize)> {
    let mut end = pos;
    while end + 1 < input.len() {
        if input[end] == b'\r' && input[end + 1] == b'\n' {
            let s = std::str::from_utf8(&input[pos..end]).ok()?.to_string();
            return Some((s, end + 2));
        }
        end += 1;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn encode_simple_string() {
        assert_eq!(encode(&RespValue::SimpleString("OK".into())), b"+OK\r\n");
    }
    #[test] fn encode_error() {
        assert_eq!(encode(&RespValue::Error("ERR something".into())), b"-ERR something\r\n");
    }
    #[test] fn encode_integer() {
        assert_eq!(encode(&RespValue::Integer(42)), b":42\r\n");
        assert_eq!(encode(&RespValue::Integer(-1)), b":-1\r\n");
    }
    #[test] fn encode_bulk_string() {
        assert_eq!(encode(&RespValue::BulkString(Some(b"hello".to_vec()))), b"$5\r\nhello\r\n");
    }
    #[test] fn encode_null_bulk_string() {
        assert_eq!(encode(&RespValue::BulkString(None)), b"$-1\r\n");
    }
    #[test] fn encode_array() {
        let v = RespValue::Array(Some(vec![
            RespValue::Integer(1),
            RespValue::Integer(2),
            RespValue::BulkString(Some(b"foo".to_vec())),
        ]));
        let e = encode(&v);
        assert_eq!(e, b"*3\r\n:1\r\n:2\r\n$3\r\nfoo\r\n");
    }
    #[test] fn encode_null_array() {
        assert_eq!(encode(&RespValue::Array(None)), b"*-1\r\n");
    }
    #[test] fn parse_simple_string() {
        let (v, n) = parse(b"+OK\r\n").unwrap();
        assert_eq!(v, RespValue::SimpleString("OK".into()));
        assert_eq!(n, 5);
    }
    #[test] fn parse_integer() {
        let (v, n) = parse(b":1234\r\n").unwrap();
        assert_eq!(v, RespValue::Integer(1234));
        assert_eq!(n, 7);
    }
    #[test] fn parse_bulk_string() {
        let (v, n) = parse(b"$5\r\nhello\r\n").unwrap();
        assert_eq!(v, RespValue::BulkString(Some(b"hello".to_vec())));
        assert_eq!(n, 11);
    }
    #[test] fn parse_null_bulk() {
        let (v, _) = parse(b"$-1\r\n").unwrap();
        assert_eq!(v, RespValue::BulkString(None));
    }
    #[test] fn parse_array() {
        let (v, _) = parse(b"*2\r\n$3\r\nfoo\r\n:42\r\n").unwrap();
        assert_eq!(v, RespValue::Array(Some(vec![
            RespValue::BulkString(Some(b"foo".to_vec())),
            RespValue::Integer(42),
        ])));
    }
    #[test] fn parse_null_array() {
        let (v, _) = parse(b"*-1\r\n").unwrap();
        assert_eq!(v, RespValue::Array(None));
    }
    #[test] fn encode_decode_roundtrip() {
        let original = RespValue::Array(Some(vec![
            RespValue::BulkString(Some(b"GET".to_vec())),
            RespValue::BulkString(Some(b"key".to_vec())),
        ]));
        let encoded = encode(&original);
        let (decoded, _) = parse(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
    #[test] fn parse_error() {
        let (v, _) = parse(b"-ERR something\r\n").unwrap();
        assert_eq!(v, RespValue::Error("ERR something".into()));
    }
    #[test] fn parse_empty_fails() {
        assert_eq!(parse(&[]), Err(ParseError::Empty));
    }
    #[test] fn parse_incomplete_fails() {
        assert!(parse(b"+OK").is_err());
    }
}
