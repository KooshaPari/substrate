pub fn b64url_encode(data: &[u8]) -> String {
    use std::str;
    let enc = base64_encode(data);
    enc.replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}
pub fn b64url_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut s = s.replace('-', "+").replace('_', "/");
    let pad = (4 - s.len() % 4) % 4;
    s.push_str(&"=".repeat(pad));
    base64_decode(&s)
}
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let b = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push(T[((b >> 6) & 0x3f) as usize] as char);
        out.push(T[(b & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let b = (data[i] as u32) << 16;
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(T[((b >> 18) & 0x3f) as usize] as char);
        out.push(T[((b >> 12) & 0x3f) as usize] as char);
        out.push(T[((b >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        let v: u32 = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => continue,
            _ => return Err(format!("bad char {}", b as char)),
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let block = 64usize;
    let key: Vec<u8> = if key.len() > block { sha256(key).to_vec() } else { key.to_vec() };
    let mut k = vec![0u8; block];
    for (i, b) in key.iter().enumerate() { k[i] = *b; }
    let mut ipad = vec![0x36u8; block];
    let mut opad = vec![0x5cu8; block];
    for i in 0..block {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = sha256_concat(&ipad, msg);
    sha256_concat(&opad, &inner)
}
fn sha256_concat(a: &[u8], b: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(a.len() + b.len());
    buf.extend_from_slice(a);
    buf.extend_from_slice(b);
    sha256(&buf)
}
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    const H0: [u32; 8] = [0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
    let mut h = H0;
    let bit_len = (data.len() as u64) * 8;
    let mut buf = data.to_vec();
    buf.push(0x80);
    while buf.len() % 64 != 56 { buf.push(0); }
    buf.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in buf.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let mut a = h[0]; let mut b = h[1]; let mut c = h[2]; let mut d = h[3];
        let mut e = h[4]; let mut f = h[5]; let mut g = h[6]; let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let mj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(mj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() { out[i*4..i*4+4].copy_from_slice(&v.to_be_bytes()); }
    out
}

pub fn encode_hs256(header_json: &str, payload_json: &str, secret: &[u8]) -> String {
    let h = b64url_encode(header_json.as_bytes());
    let p = b64url_encode(payload_json.as_bytes());
    let signing = format!("{}.{}", h, p);
    let mac = hmac_sha256(secret, signing.as_bytes());
    let s = b64url_encode(&mac);
    format!("{}.{}", signing, s)
}

pub fn verify_hs256(token: &str, secret: &[u8]) -> Result<(String, String), String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 { return Err("expected 3 parts".into()); }
    let signing = format!("{}.{}", parts[0], parts[1]);
    let expected = hmac_sha256(secret, signing.as_bytes());
    let got = b64url_decode(parts[2])?;
    if got.len() != expected.len() { return Err("bad sig".into()); }
    let mut diff = 0u8;
    for (a, b) in got.iter().zip(expected.iter()) { diff |= a ^ b; }
    if diff != 0 { return Err("bad sig".into()); }
    let header = String::from_utf8(b64url_decode(parts[0])?).map_err(|_| "header utf8")?;
    let payload = String::from_utf8(b64url_decode(parts[1])?).map_err(|_| "payload utf8")?;
    Ok((header, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn b64url_roundtrip() {
        let s = b64url_encode(b"hello world");
        let back = b64url_decode(&s).unwrap();
        assert_eq!(back, b"hello world");
    }
    #[test] fn b64url_no_pad() { assert!(!b64url_encode(b"a").contains('=')); }
    #[test] fn sha256_known() {
        // SHA256("abc")
        let h = sha256(b"abc");
        let hex: String = h.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
    #[test] fn hmac_rfc4231() {
        // RFC 4231 test case 1: key="key", msg="The quick brown fox jumps over the lazy dog"
        let m = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
        let hex: String = m.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
    }
    #[test] fn jwt_roundtrip() {
        let tok = encode_hs256(r##"{"alg":"HS256","typ":"JWT"}"##, r#"{"sub":"alice"}"#, b"secret");
        let (h, p) = verify_hs256(&tok, b"secret").unwrap();
        assert!(h.contains("HS256"));
        assert!(p.contains("alice"));
    }
    #[test] fn jwt_bad_sig() {
        let tok = encode_hs256(r##"{"alg":"HS256"}"##, r#"{"x":1}"#, b"secret");
        let r = verify_hs256(&tok, b"wrong");
        assert!(r.is_err());
    }
    #[test] fn jwt_tampered() {
        let tok = encode_hs256(r##"{"alg":"HS256"}"##, r#"{"x":1}"#, b"secret");
        let parts: Vec<&str> = tok.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], parts[1], "AAAA");
        assert!(verify_hs256(&tampered, b"secret").is_err());
    }
}
