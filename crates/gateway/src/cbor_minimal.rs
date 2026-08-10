// Minimal CBOR encoder/decoder. Supports the major types 0..=5 (uint, nint, bstr,
// tstr, array, map). Major types 6 and 7 (tags, floats/special) are parsed only
// for floats and bool/null — the rest falls under `Unknown`. Output is
// `Vec<(Value, usize)>`-shaped, returning the consumed byte count.
//
// This is NOT a full RFC 8949 implementation — use the `ciborium` crate for that.

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    UInt(u64),
    NInt(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
    Float(f64),
    Unknown(u8, u64),
}

pub fn encode(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::UInt(n) => encode_head(0, *n, out),
        Value::NInt(n) => encode_head(1, (-1 - *n) as u64, out),
        Value::Bytes(b) => {
            encode_head(2, b.len() as u64, out);
            out.extend_from_slice(b);
        }
        Value::Text(s) => {
            encode_head(3, s.len() as u64, out);
            out.extend_from_slice(s.as_bytes());
        }
        Value::Array(items) => {
            encode_head(4, items.len() as u64, out);
            for item in items {
                encode(item, out);
            }
        }
        Value::Map(entries) => {
            encode_head(5, entries.len() as u64, out);
            for (k, v) in entries {
                encode(k, out);
                encode(v, out);
            }
        }
        Value::Bool(true) => {
            out.push(0xf5);
        }
        Value::Bool(false) => {
            out.push(0xf4);
        }
        Value::Null => {
            out.push(0xf6);
        }
        Value::Float(f) => {
            out.push(0xfb);
            out.extend_from_slice(&f.to_be_bytes());
        }
        Value::Unknown(_, _) => {
            out.push(0xff);
        }
    }
}

fn encode_head(major: u8, arg: u64, out: &mut Vec<u8>) {
    let lead = major << 5;
    if arg < 24 {
        out.push(lead | arg as u8);
    } else if arg < 0x100 {
        out.push(lead | 24);
        out.push(arg as u8);
    } else if arg < 0x10000 {
        out.push(lead | 25);
        out.extend_from_slice(&(arg as u16).to_be_bytes());
    } else if arg < 0x100000000 {
        out.push(lead | 26);
        out.extend_from_slice(&(arg as u32).to_be_bytes());
    } else {
        out.push(lead | 27);
        out.extend_from_slice(&arg.to_be_bytes());
    }
}

pub fn decode(input: &[u8]) -> Result<(Value, usize), String> {
    if input.is_empty() {
        return Err("empty input".into());
    }
    let head = input[0];
    let major = head >> 5;
    let info = head & 0x1f;
    let (arg, head_size) = read_arg(info, input)?;
    match major {
        0 => Ok((Value::UInt(arg), head_size)),
        1 => Ok((Value::NInt(-(arg as i64) - 1), head_size)),
        2 => {
            if input.len() < head_size + arg as usize {
                return Err("bstr truncated".into());
            }
            let bytes = input[head_size..head_size + arg as usize].to_vec();
            Ok((Value::Bytes(bytes), head_size + arg as usize))
        }
        3 => {
            if input.len() < head_size + arg as usize {
                return Err("tstr truncated".into());
            }
            let raw = &input[head_size..head_size + arg as usize];
            let s = std::str::from_utf8(raw)
                .map_err(|_| "bad utf8")?
                .to_string();
            Ok((Value::Text(s), head_size + arg as usize))
        }
        4 => {
            let mut pos = head_size;
            let mut items = Vec::with_capacity(arg as usize);
            for _ in 0..arg {
                let (v, n) = decode(&input[pos..])?;
                items.push(v);
                pos += n;
            }
            Ok((Value::Array(items), pos))
        }
        5 => {
            let mut pos = head_size;
            let mut entries = Vec::with_capacity(arg as usize);
            for _ in 0..arg {
                let (k, n) = decode(&input[pos..])?;
                pos += n;
                let (v, n) = decode(&input[pos..])?;
                pos += n;
                entries.push((k, v));
            }
            Ok((Value::Map(entries), pos))
        }
        7 => match info {
            20 => Ok((Value::Bool(false), 1)),
            21 => Ok((Value::Bool(true), 1)),
            22 => Ok((Value::Null, 1)),
            27 => {
                if input.len() < 9 {
                    return Err("float truncated".into());
                }
                let bytes: [u8; 8] = input[1..9].try_into().map_err(|_| "bad float")?;
                Ok((Value::Float(f64::from_be_bytes(bytes)), 9))
            }
            26 => {
                if input.len() < 5 {
                    return Err("float32 truncated".into());
                }
                let bytes: [u8; 4] = input[1..5].try_into().map_err(|_| "bad float32")?;
                Ok((Value::Float(f32::from_be_bytes(bytes) as f64), 5))
            }
            25 => {
                if input.len() < 3 {
                    return Err("float16 truncated".into());
                }
                Ok((Value::Unknown(major, arg), 3))
            }
            other => Ok((Value::Unknown(major, other as u64), head_size)),
        },
        _ => Ok((Value::Unknown(major, arg), head_size)),
    }
}

fn read_arg(info: u8, input: &[u8]) -> Result<(u64, usize), String> {
    match info {
        x if x < 24 => Ok((x as u64, 1)),
        24 => {
            if input.len() < 2 {
                return Err("u8 truncated".into());
            }
            Ok((input[1] as u64, 2))
        }
        25 => {
            if input.len() < 3 {
                return Err("u16 truncated".into());
            }
            Ok((u16::from_be_bytes([input[1], input[2]]) as u64, 3))
        }
        26 => {
            if input.len() < 5 {
                return Err("u32 truncated".into());
            }
            Ok((
                u32::from_be_bytes([input[1], input[2], input[3], input[4]]) as u64,
                5,
            ))
        }
        27 => {
            if input.len() < 9 {
                return Err("u64 truncated".into());
            }
            let v = u64::from_be_bytes(input[1..9].try_into().map_err(|_| "bad u64")?);
            Ok((v, 9))
        }
        31 => Err("indefinite length not supported".into()),
        _ => Err(format!("invalid info {}", info)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rt(v: &Value) -> Value {
        let mut buf = Vec::new();
        encode(v, &mut buf);
        let (out, n) = decode(&buf).unwrap();
        assert_eq!(n, buf.len());
        out
    }
    #[test]
    fn small_uint() {
        assert_eq!(rt(&Value::UInt(5)), Value::UInt(5));
    }
    #[test]
    fn large_uint() {
        assert_eq!(rt(&Value::UInt(1_000_000)), Value::UInt(1_000_000));
    }
    #[test]
    fn huge_uint() {
        assert_eq!(rt(&Value::UInt(u64::MAX)), Value::UInt(u64::MAX));
    }
    #[test]
    fn nint() {
        assert_eq!(rt(&Value::NInt(-1)), Value::NInt(-1));
        assert_eq!(rt(&Value::NInt(-1000)), Value::NInt(-1000));
    }
    #[test]
    fn text() {
        assert_eq!(
            rt(&Value::Text("hello".into())),
            Value::Text("hello".into())
        );
    }
    #[test]
    fn long_text() {
        let s = "x".repeat(1000);
        assert_eq!(rt(&Value::Text(s.clone())), Value::Text(s));
    }
    #[test]
    fn bytes() {
        let b = vec![1, 2, 3, 4, 5];
        assert_eq!(rt(&Value::Bytes(b.clone())), Value::Bytes(b));
    }
    #[test]
    fn array() {
        let a = Value::Array(vec![Value::UInt(1), Value::UInt(2), Value::UInt(3)]);
        assert_eq!(rt(&a), a);
    }
    #[test]
    fn nested_array() {
        let a = Value::Array(vec![Value::UInt(1), Value::Array(vec![Value::UInt(2)])]);
        assert_eq!(rt(&a), a);
    }
    #[test]
    fn map() {
        let m = Value::Map(vec![(Value::Text("a".into()), Value::UInt(1))]);
        assert_eq!(rt(&m), m);
    }
    #[test]
    fn bools_null() {
        assert_eq!(rt(&Value::Bool(true)), Value::Bool(true));
        assert_eq!(rt(&Value::Bool(false)), Value::Bool(false));
        assert_eq!(rt(&Value::Null), Value::Null);
    }
    #[test]
    fn float() {
        let v = Value::Float(3.14);
        let mut buf = Vec::new();
        encode(&v, &mut buf);
        let (out, _) = decode(&buf).unwrap();
        if let Value::Float(f) = out {
            assert_eq!(f, 3.14);
        } else {
            panic!();
        }
    }
    #[test]
    fn empty_array() {
        assert_eq!(rt(&Value::Array(vec![])), Value::Array(vec![]));
    }
    #[test]
    fn empty_map() {
        assert_eq!(rt(&Value::Map(vec![])), Value::Map(vec![]));
    }
    #[test]
    fn rejects_indefinite() {
        let buf = vec![0x1f];
        assert!(decode(&buf).is_err());
    }
    #[test]
    fn rejects_truncated() {
        let buf = vec![0x19];
        assert!(decode(&buf).is_err());
    }
}
