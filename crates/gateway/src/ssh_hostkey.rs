// Minimal SSH wire-format parser (RFC 4253 §6). Reads SSH public key blobs of
// the form:  string  "ssh-ed25519" / "ssh-rsa" / "ecdsa-sha2-nistp256" ...
// followed by the key-type-specific payload.
//
// Does NOT verify signatures. Use the `ssh-key` crate or `russh` for full
// trust + verification. This module is enough to display host keys and
// compute their SSH-style fingerprint ("SHA256:...").

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum HostKey {
    Ed25519 { key: [u8; 32] },
    Rsa { e: Vec<u8>, n: Vec<u8> },
    EcdsaNistp256 { curve: String, q: Vec<u8> },
    Unknown { key_type: String, blob: Vec<u8> },
}

pub fn parse(input: &[u8]) -> Result<HostKey, String> {
    let (key_type, rest) = read_string(input)?;
    let key_type_str = std::str::from_utf8(key_type).map_err(|_| "bad utf8 in key type")?;
    match key_type_str {
        "ssh-ed25519" => {
            let (key_bytes, _) = read_string(rest)?;
            if key_bytes.len() != 32 {
                return Err("ed25519 key must be 32 bytes".into());
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(key_bytes);
            Ok(HostKey::Ed25519 { key })
        }
        "ssh-rsa" => {
            let (e, rest) = read_string(rest)?;
            let (n, _) = read_string(rest)?;
            Ok(HostKey::Rsa {
                e: e.to_vec(),
                n: n.to_vec(),
            })
        }
        "ecdsa-sha2-nistp256" => {
            let (curve, rest) = read_string(rest)?;
            let (q, _) = read_string(rest)?;
            Ok(HostKey::EcdsaNistp256 {
                curve: std::str::from_utf8(curve)
                    .map_err(|_| "bad curve name")?
                    .to_string(),
                q: q.to_vec(),
            })
        }
        other => Ok(HostKey::Unknown {
            key_type: other.to_string(),
            blob: rest.to_vec(),
        }),
    }
}

fn read_string(input: &[u8]) -> Result<(&[u8], &[u8]), String> {
    if input.len() < 4 {
        return Err("string length truncated".into());
    }
    let len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if input.len() < 4 + len {
        return Err("string value truncated".into());
    }
    Ok((&input[4..4 + len], &input[4 + len..]))
}

pub fn fingerprint_sha256(input: &[u8]) -> String {
    let hash = sha256(input);
    let b64 = base64_no_pad(&hash);
    format!("SHA256:{}", b64)
}

fn sha256(input: &[u8]) -> [u8; 32] {
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let k = [
        0x428a2f98u32,
        0x71374491,
        0xb5c0fbcf,
        0xe9b5dba5,
        0x3956c25b,
        0x59f111f1,
        0x923f82a4,
        0xab1c5ed5,
        0xd807aa98,
        0x12835b01,
        0x243185be,
        0x550c7dc3,
        0x72be5d74,
        0x80deb1fe,
        0x9bdc06a7,
        0xc19bf174,
        0xe49b69c1,
        0xefbe4786,
        0x0fc19dc6,
        0x240ca1cc,
        0x2de92c6f,
        0x4a7484aa,
        0x5cb0a9dc,
        0x76f988da,
        0x983e5152,
        0xa831c66d,
        0xb00327c8,
        0xbf597fc7,
        0xc6e00bf3,
        0xd5a79147,
        0x06ca6351,
        0x14292967,
        0x27b70a85,
        0x2e1b2138,
        0x4d2c6dfc,
        0x53380d13,
        0x650a7354,
        0x766a0abb,
        0x81c2c92e,
        0x92722c85,
        0xa2bfe8a1,
        0xa81a664b,
        0xc24b8b70,
        0xc76c51a3,
        0xd192e819,
        0xd6990624,
        0xf40e3585,
        0x106aa070,
        0x19a4c116,
        0x1e376c08,
        0x2748774c,
        0x34b0bcb5,
        0x391c0cb3,
        0x4ed8aa4a,
        0x5b9cca4f,
        0x682e6ff3,
        0x748f82ee,
        0x78a5636f,
        0x84c87814,
        0x8cc70208,
        0x90befffa,
        0xa4506ceb,
        0xbef9a3f7,
        0xc67178f2,
    ];
    let mut msg = input.to_vec();
    let bit_len = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let mj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(mj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn base64_no_pad(input: &[u8]) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= input.len() {
        let b0 = input[i];
        let b1 = input[i + 1];
        let b2 = input[i + 2];
        out.push(alphabet[(b0 >> 2) as usize] as char);
        out.push(alphabet[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(alphabet[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(alphabet[(b2 & 0x3f) as usize] as char);
        i += 3;
    }
    if i < input.len() {
        let b0 = input[i];
        out.push(alphabet[(b0 >> 2) as usize] as char);
        if i + 1 < input.len() {
            let b1 = input[i + 1];
            out.push(alphabet[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(alphabet[((b1 & 0x0f) << 2) as usize] as char);
        } else {
            out.push(alphabet[((b0 & 0x03) << 4) as usize] as char);
        }
    }
    out
}

pub fn key_metadata(key: &HostKey) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    match key {
        HostKey::Ed25519 { .. } => {
            m.insert("type".into(), "ssh-ed25519".into());
        }
        HostKey::Rsa { .. } => {
            m.insert("type".into(), "ssh-rsa".into());
        }
        HostKey::EcdsaNistp256 { curve, .. } => {
            m.insert("type".into(), "ecdsa-sha2".into());
            m.insert("curve".into(), curve.clone());
        }
        HostKey::Unknown { key_type, .. } => {
            m.insert("type".into(), key_type.clone());
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mk_ed25519_blob(key: &[u8; 32]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&11u32.to_be_bytes());
        v.extend_from_slice(b"ssh-ed25519");
        v.extend_from_slice(&32u32.to_be_bytes());
        v.extend_from_slice(key);
        v
    }
    #[test]
    fn parse_ed25519() {
        let key = [0x42u8; 32];
        let blob = mk_ed25519_blob(&key);
        let h = parse(&blob).unwrap();
        assert_eq!(h, HostKey::Ed25519 { key });
    }
    #[test]
    fn parse_short_truncated() {
        assert!(parse(&[0, 0, 0, 11]).is_err());
    }
    #[test]
    fn parse_unknown_type() {
        let mut v = Vec::new();
        v.extend_from_slice(&9u32.to_be_bytes());
        v.extend_from_slice(b"ssh-dss");
        v.extend_from_slice(&[1, 2, 3]);
        let h = parse(&v).unwrap();
        assert!(matches!(h, HostKey::Unknown { .. }));
    }
    #[test]
    fn fingerprint_format() {
        let key = [0x01u8; 32];
        let blob = mk_ed25519_blob(&key);
        let fp = fingerprint_sha256(&blob);
        assert!(fp.starts_with("SHA256:"));
        assert_eq!(fp.len(), "SHA256:".len() + 43);
    }
    #[test]
    fn metadata_for_ed25519() {
        let key = [0u8; 32];
        let h = HostKey::Ed25519 { key };
        let m = key_metadata(&h);
        assert_eq!(m.get("type").unwrap(), "ssh-ed25519");
    }
    #[test]
    fn parse_rsa() {
        let mut v = Vec::new();
        v.extend_from_slice(&7u32.to_be_bytes());
        v.extend_from_slice(b"ssh-rsa");
        v.extend_from_slice(&3u32.to_be_bytes());
        v.extend_from_slice(&[1, 0, 1]); // e
        v.extend_from_slice(&3u32.to_be_bytes());
        v.extend_from_slice(&[0, 1, 2]); // n
        let h = parse(&v).unwrap();
        if let HostKey::Rsa { e, n } = h {
            assert_eq!(e, vec![1, 0, 1]);
            assert_eq!(n, vec![0, 1, 2]);
        } else {
            panic!();
        }
    }
    #[test]
    fn parse_ecdsa() {
        let mut v = Vec::new();
        v.extend_from_slice(&19u32.to_be_bytes());
        v.extend_from_slice(b"ecdsa-sha2-nistp256");
        v.extend_from_slice(&8u32.to_be_bytes());
        v.extend_from_slice(b"nistp256");
        v.extend_from_slice(&4u32.to_be_bytes());
        v.extend_from_slice(&[1, 2, 3, 4]);
        let h = parse(&v).unwrap();
        if let HostKey::EcdsaNistp256 { curve, q } = h {
            assert_eq!(curve, "nistp256");
            assert_eq!(q, vec![1, 2, 3, 4]);
        } else {
            panic!();
        }
    }
    #[test]
    fn ed25519_wrong_length() {
        let mut v = Vec::new();
        v.extend_from_slice(&11u32.to_be_bytes());
        v.extend_from_slice(b"ssh-ed25519");
        v.extend_from_slice(&16u32.to_be_bytes());
        v.extend_from_slice(&[0u8; 16]);
        assert!(parse(&v).is_err());
    }
    #[test]
    fn base64_no_pad_short() {
        assert_eq!(base64_no_pad(&[0xaa, 0xbb]), "qrs");
        assert_eq!(base64_no_pad(&[0xaa]), "qg");
    }
}
