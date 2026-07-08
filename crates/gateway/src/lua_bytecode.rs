// Minimal Lua 5.4 bytecode header parser. Reads the binary chunk header fields
// (magic, version, format, endianness, sizes, source name, line counts) and the
// top-level function prototype header. Does NOT decode individual instructions.
//
// Refs: Lua 5.4 reference manual §3 ("The Lua Binary Chunk Format").
// Use the `luau` or `lua-src` crate for full decoding.

pub const LUA_MAGIC: [u8; 4] = [0x1b, b'L', b'u', b'a'];

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ChunkHeader {
    pub magic: [u8; 4],
    pub version: u8,
    pub format: u8,
    pub endianness: u8,
    pub int_size: u8,
    pub size_t_size: u8,
    pub instruction_size: u8,
    pub number_size: u8,
    pub number_integral: u8,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Upvalue {
    pub instack: bool,
    pub idx: u8,
    pub kind: u8,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Constant {
    pub kind: u8,
    pub value: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FunctionProto {
    pub source: String,
    pub line_defined: i32,
    pub last_line_defined: i32,
    pub num_params: u8,
    pub is_vararg: bool,
    pub max_stack_size: u8,
    pub constants: Vec<Constant>,
    pub upvalues: Vec<Upvalue>,
}

pub fn parse_chunk_header(input: &[u8]) -> Result<(ChunkHeader, &[u8]), String> {
    if input.len() < 12 { return Err("chunk too short".into()); }
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&input[0..4]);
    if magic != LUA_MAGIC { return Err("not a Lua chunk".into()); }
    let header = ChunkHeader {
        magic,
        version: input[4],
        format: input[5],
        endianness: input[6],
        int_size: input[7],
        size_t_size: input[8],
        instruction_size: input[9],
        number_size: input[10],
        number_integral: input[11],
    };
    Ok((header, &input[12..]))
}

pub fn parse_function_proto(input: &[u8]) -> Result<(FunctionProto, &[u8]), String> {
    let (source_raw, rest) = read_string(input)?;
    let source = std::str::from_utf8(&source_raw).map_err(|_| "bad utf8 in source")?.to_string();
    let (line_defined, rest) = read_int(rest)?;
    let (last_line_defined, rest) = read_int(rest)?;
    let (num_params, rest) = read_byte(rest)?;
    let (is_vararg, rest) = read_byte(rest)?;
    let (max_stack, rest) = read_byte(rest)?;
    let (code_bytes, rest) = read_int_leb(rest)?;
    let rest = &rest[code_bytes as usize..];
    let (const_count, mut rest) = read_int_leb(rest)?;
    let mut constants = Vec::with_capacity(const_count as usize);
    for _ in 0..const_count {
        let (kind, r) = read_byte(rest)?;
        let (value, r) = read_const_value(kind, r)?;
        constants.push(Constant { kind, value });
        rest = r;
    }
    let (upval_count, mut rest) = read_int_leb(rest)?;
    let mut upvalues = Vec::with_capacity(upval_count as usize);
    for _ in 0..upval_count {
        let (instack, r) = read_byte(rest)?;
        let (idx, r) = read_byte(r)?;
        let (kind, r) = read_byte(r)?;
        upvalues.push(Upvalue { instack: instack != 0, idx, kind });
        rest = r;
    }
    Ok((FunctionProto {
        source, line_defined, last_line_defined,
        num_params, is_vararg: is_vararg != 0, max_stack_size: max_stack,
        constants, upvalues,
    }, rest))
}

fn read_string(input: &[u8]) -> Result<(Vec<u8>, &[u8]), String> {
    if input.is_empty() { return Err("empty".into()); }
    let (size, rest) = read_size(input)?;
    if rest.len() < size as usize { return Err("string truncated".into()); }
    Ok((rest[..size as usize].to_vec(), &rest[size as usize..]))
}

fn read_size(input: &[u8]) -> Result<(u64, &[u8]), String> {
    if input.is_empty() { return Err("empty".into()); }
    let first = input[0];
    let mut x: u64 = first as u64;
    if x == 0xFF {
        if input.len() < 9 { return Err("bad size".into()); }
        let bytes: [u8; 8] = input[1..9].try_into().map_err(|_| "bad size")?;
        return Ok((u64::from_be_bytes(bytes), &input[9..]));
    }
    Ok((x, &input[1..]))
}

fn read_int(input: &[u8]) -> Result<(i32, &[u8]), String> {
    if input.len() < 4 { return Err("int truncated".into()); }
    Ok((i32::from_be_bytes([input[0], input[1], input[2], input[3]]), &input[4..]))
}

fn read_byte(input: &[u8]) -> Result<(u8, &[u8]), String> {
    if input.is_empty() { return Err("byte truncated".into()); }
    Ok((input[0], &input[1..]))
}

fn read_int_leb(input: &[u8]) -> Result<(i64, &[u8]), String> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut consumed = 0;
    for &b in input {
        consumed += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 { break; }
        shift += 7;
        if shift > 63 { return Err("LEB too long".into()); }
    }
    Ok((result as i64, &input[consumed..]))
}

fn read_const_value(kind: u8, input: &[u8]) -> Result<(Vec<u8>, &[u8]), String> {
    match kind {
        0x00 => Ok((vec![], input)),
        0x01 => {
            if input.is_empty() { return Err("nil truncated".into()); }
            Ok((vec![input[0]], &input[1..]))
        }
        0x03 => Ok((input[..4].to_vec(), &input[4..])),
        0x13 => Ok((input[..8].to_vec(), &input[8..])),
        0x04 => read_string(input),
        _ => Err(format!("unsupported const kind 0x{:02x}", kind)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mk_header(version: u8, format: u8) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&LUA_MAGIC);
        v.push(version);
        v.push(format);
        v.push(1); // little endian
        v.push(4);
        v.push(8);
        v.push(4);
        v.push(8);
        v.push(0);
        v
    }
    #[test] fn parse_header_basic() {
        let h = mk_header(0x54, 0);
        let (hdr, rest) = parse_chunk_header(&h).unwrap();
        assert_eq!(hdr.version, 0x54);
        assert_eq!(hdr.format, 0);
        assert_eq!(hdr.endianness, 1);
        assert_eq!(rest.len(), 0);
    }
    #[test] fn bad_magic() {
        let v = vec![0, 1, 2, 3, 0x54, 0, 1, 4, 8, 4, 8, 0];
        assert!(parse_chunk_header(&v).is_err());
    }
    #[test] fn short_chunk() {
        assert!(parse_chunk_header(&[0u8; 5]).is_err());
    }
    #[test] fn read_size_short_form() {
        let (size, rest) = read_size(&[5, 0xaa, 0xbb]).unwrap();
        assert_eq!(size, 5);
        assert_eq!(rest, &[0xaa, 0xbb]);
    }
    #[test] fn read_size_long_form() {
        let mut v = vec![0xff];
        v.extend_from_slice(&42u64.to_be_bytes());
        let (size, _) = read_size(&v).unwrap();
        assert_eq!(size, 42);
    }
    #[test] fn read_int_basic() {
        let (n, _) = read_int(&0x12345678u32.to_be_bytes()).unwrap();
        assert_eq!(n, 0x12345678);
    }
    #[test] fn read_byte_basic() {
        assert_eq!(read_byte(&[0x42, 0xff]).unwrap(), (0x42, &[0xff][..]));
    }
    #[test] fn read_leb_single() {
        assert_eq!(read_int_leb(&[0x05]).unwrap(), (5, &[][..]));
    }
    #[test] fn read_leb_multi() {
        // 641 = 1 + (5 << 7) -- first byte contributes 1, second byte contributes 5<<7 = 640
        assert_eq!(read_int_leb(&[0x80 | 1, 0x05]).unwrap(), (641, &[][..]));
    }
    #[test] fn read_string_short() {
        let v = vec![3, b'a', b'b', b'c', 0xff];
        let (s, rest) = read_string(&v).unwrap();
        assert_eq!(s, b"abc");
        assert_eq!(rest, &[0xff]);
    }
    #[test] fn read_string_zero() {
        let v = vec![0, 0xff];
        let (s, _) = read_string(&v).unwrap();
        assert!(s.is_empty());
    }
    #[test] fn parse_function_proto_minimal() {
        // source: "test" (size 4), line_defined=10, last_line=20,
        // num_params=0, is_vararg=0, max_stack=2
        // code: size 0 (no instructions)
        // consts: count 0
        // upvalues: count 0
        let mut v = vec![];
        v.push(4); v.extend_from_slice(b"test");
        v.extend_from_slice(&10i32.to_be_bytes());
        v.extend_from_slice(&20i32.to_be_bytes());
        v.push(0); v.push(0); v.push(2);
        v.push(0); // code size 0
        v.push(0); // const count 0
        v.push(0); // upval count 0
        let (proto, _) = parse_function_proto(&v).unwrap();
        assert_eq!(proto.source, "test");
        assert_eq!(proto.line_defined, 10);
        assert_eq!(proto.num_params, 0);
        assert_eq!(proto.max_stack_size, 2);
        assert!(!proto.is_vararg);
        assert!(proto.constants.is_empty());
        assert!(proto.upvalues.is_empty());
    }
    #[test] fn parse_full_chunk() {
        let mut v = mk_header(0x54, 0);
        v.push(4); v.extend_from_slice(b"main");
        v.extend_from_slice(&1i32.to_be_bytes());
        v.extend_from_slice(&100i32.to_be_bytes());
        v.push(1); v.push(0); v.push(5);
        v.push(0); // code
        v.push(0); // consts
        v.push(0); // upvalues
        let (h, rest) = parse_chunk_header(&v).unwrap();
        assert_eq!(h.version, 0x54);
        let (proto, _) = parse_function_proto(rest).unwrap();
        assert_eq!(proto.source, "main");
        assert_eq!(proto.num_params, 1);
        assert_eq!(proto.max_stack_size, 5);
    }
    #[test] fn parse_function_with_const_and_upval() {
        let mut v = vec![];
        v.push(1); v.extend_from_slice(b"x");
        v.extend_from_slice(&1i32.to_be_bytes());
        v.extend_from_slice(&1i32.to_be_bytes());
        v.push(0); v.push(0); v.push(1);
        v.push(0); // code size
        v.push(1); // 1 const
        v.push(0x04); // kind = string
        v.push(3); v.extend_from_slice(b"foo");
        v.push(1); // 1 upvalue
        v.push(1); v.push(0); v.push(0);
        let (proto, _) = parse_function_proto(&v).unwrap();
        assert_eq!(proto.constants.len(), 1);
        assert_eq!(proto.constants[0].kind, 0x04);
        assert_eq!(proto.constants[0].value, b"foo");
        assert_eq!(proto.upvalues.len(), 1);
        assert!(proto.upvalues[0].instack);
        assert_eq!(proto.upvalues[0].idx, 0);
    }
    #[test] fn rejects_bad_size() {
        let v = vec![0xff, 0, 0];
        assert!(read_size(&v).is_err());
    }
}