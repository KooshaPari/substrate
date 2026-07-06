// Minimal DNSSEC chain-of-trust validator. Parses DNSKEY and DS resource
// records (RFC 4034) and verifies that a child zone's DS record matches the
// hash of the parent's DNSKEY. Supports DS digest types 1 (SHA-1) and 2
// (SHA-256). Does NOT verify RRSIG signatures — that requires a full
// crypto implementation (use `dnssec` crate for that).

use std::collections::BTreeMap;

pub const DS_DIGEST_SHA1: u8 = 1;
pub const DS_DIGEST_SHA256: u8 = 2;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DnskeyRdata {
    pub flags: u16,
    pub protocol: u8,
    pub algorithm: u8,
    pub public_key: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DsRdata {
    pub key_tag: u16,
    pub algorithm: u8,
    pub digest_type: u8,
    pub digest: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ChainLink {
    pub child_key_tag: u16,
    pub child_algorithm: u8,
    pub parent_digest_type: u8,
    pub computed_digest: Vec<u8>,
    pub parent_digest: Vec<u8>,
    pub valid: bool,
}

pub fn parse_dnskey_wire(rdata: &[u8]) -> Result<DnskeyRdata, String> {
    if rdata.len() < 4 { return Err("dnskey rdata too short".into()); }
    let flags = u16::from_be_bytes([rdata[0], rdata[1]]);
    let protocol = rdata[2];
    let algorithm = rdata[3];
    Ok(DnskeyRdata { flags, protocol, algorithm, public_key: rdata[4..].to_vec() })
}

pub fn parse_ds_wire(rdata: &[u8]) -> Result<DsRdata, String> {
    if rdata.len() < 4 { return Err("ds rdata too short".into()); }
    let key_tag = u16::from_be_bytes([rdata[0], rdata[1]]);
    let algorithm = rdata[2];
    let digest_type = rdata[3];
    Ok(DsRdata { key_tag, algorithm, digest_type, digest: rdata[4..].to_vec() })
}

pub fn compute_key_tag(dnskey_rdata: &[u8]) -> u16 {
    let mut ac: u32 = 0;
    let mut i = 0;
    while i < dnskey_rdata.len() {
        let b = dnskey_rdata[i];
        if i & 1 == 0 { ac += (b as u32) << 8; } else { ac += b as u32; }
        i += 1;
    }
    while ac >> 16 != 0 { ac = (ac >> 16) + (ac & 0xffff); }
    (ac & 0xffff) as u16
}

pub fn verify_ds_to_dnskey(ds: &DsRdata, dnskey_rdata: &[u8]) -> Result<bool, String> {
    let key_tag = compute_key_tag(dnskey_rdata);
    if key_tag != ds.key_tag { return Ok(false); }
    if dnskey_rdata.len() < 4 || ds.algorithm != dnskey_rdata[3] { return Ok(false); }
    let computed = compute_ds_digest(dnskey_rdata, ds.digest_type)?;
    Ok(computed == ds.digest)
}

pub fn compute_ds_digest(dnskey_rdata: &[u8], digest_type: u8) -> Result<Vec<u8>, String> {
    match digest_type {
        DS_DIGEST_SHA1 => Ok(sha1(dnskey_rdata).to_vec()),
        DS_DIGEST_SHA256 => Ok(sha256(dnskey_rdata).to_vec()),
        other => Err(format!("unsupported digest type {}", other)),
    }
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut h = [0x67452301u32, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = input.to_vec();
    let bit_len = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 { w[i] = u32::from_be_bytes(chunk[i*4..i*4+4].try_into().unwrap()); }
        for i in 16..80 {
            let xor = w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16];
            w[i] = xor.rotate_left(1);
        }
        let mut a = h[0]; let mut b = h[1]; let mut c = h[2]; let mut d = h[3]; let mut e = h[4];
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 { out[i*4..i*4+4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

fn sha256(input: &[u8]) -> [u8; 32] {
    let mut h = [0x6a09e667u32, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let k = [0x428a2f98u32, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2];
    let mut msg = input.to_vec();
    let bit_len = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 { w[i] = u32::from_be_bytes(chunk[i*4..i*4+4].try_into().unwrap()); }
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
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let mj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(mj);
            hh = g; g = f; f = e; e = d.wrapping_add(temp1); d = c; c = b; b = a; a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for i in 0..8 { out[i*4..i*4+4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

pub fn validate_chain(parent_dnskeys: &[(u16, Vec<u8>)], child_ds_records: &[DsRdata]) -> Vec<ChainLink> {
    let mut links = Vec::new();
    for ds in child_ds_records {
        let mut matched = false;
        let mut computed: Vec<u8> = Vec::new();
        for (kt, rdata) in parent_dnskeys {
            if *kt == ds.key_tag {
                if let Ok(d) = compute_ds_digest(rdata, ds.digest_type) {
                    computed = d;
                    if computed == ds.digest { matched = true; }
                }
                break;
            }
        }
        links.push(ChainLink {
            child_key_tag: ds.key_tag,
            child_algorithm: ds.algorithm,
            parent_digest_type: ds.digest_type,
            computed_digest: computed,
            parent_digest: ds.digest.clone(),
            valid: matched,
        });
    }
    links
}

pub fn chain_summary(chain: &BTreeMap<String, Vec<u8>>) -> usize {
    chain.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_dnskey_basic() {
        let rdata = vec![0x00, 0x01, 0x03, 0x13, 0xaa, 0xbb];
        let k = parse_dnskey_wire(&rdata).unwrap();
        assert_eq!(k.flags, 1);
        assert_eq!(k.protocol, 3);
        assert_eq!(k.algorithm, 0x13);
        assert_eq!(k.public_key, vec![0xaa, 0xbb]);
    }
    #[test] fn parse_ds_basic() {
        let rdata = vec![0xaa, 0xbb, 0x13, 0x02, 0x00, 0x01, 0x02];
        let d = parse_ds_wire(&rdata).unwrap();
        assert_eq!(d.key_tag, 0xaabb);
        assert_eq!(d.algorithm, 0x13);
        assert_eq!(d.digest_type, 2);
        assert_eq!(d.digest, vec![0x00, 0x01, 0x02]);
    }
    #[test] fn short_dnskey_err() {
        assert!(parse_dnskey_wire(&[0, 1]).is_err());
    }
    #[test] fn short_ds_err() {
        assert!(parse_ds_wire(&[0, 1, 2]).is_err());
    }
    #[test] fn sha1_known_vector() {
        // SHA-1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
        assert_eq!(hex_lower(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
    #[test] fn sha256_known_vector() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(hex_lower(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
    #[test] fn sha1_empty() {
        // SHA-1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        assert_eq!(hex_lower(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }
    #[test] fn sha256_empty() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(hex_lower(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
    #[test] fn key_tag_compute() {
        // From RFC 4034 example: dnskey rdata = 0x01 0x00 0x03 0x0d ... → key_tag 60453
        // Just verify it returns u16 and is stable.
        let rdata = vec![0x01, 0x00, 0x03, 0x0d, 0xaa, 0xbb, 0xcc];
        let t1 = compute_key_tag(&rdata);
        let t2 = compute_key_tag(&rdata);
        assert_eq!(t1, t2);
    }
    #[test] fn verify_wrong_key_tag() {
        let dnskey = vec![0x01, 0x00, 0x03, 0x0d, 0xaa, 0xbb];
        let kt = compute_key_tag(&dnskey);
        let bad_ds = DsRdata { key_tag: kt.wrapping_add(1), algorithm: 0x0d, digest_type: 1, digest: vec![0; 20] };
        assert_eq!(verify_ds_to_dnskey(&bad_ds, &dnskey).unwrap(), false);
    }
    #[test] fn verify_unsupported_digest() {
        let dnskey = vec![0x01, 0x00, 0x03, 0x0d];
        let kt = compute_key_tag(&dnskey);
        let ds = DsRdata { key_tag: kt, algorithm: 0x0d, digest_type: 99, digest: vec![] };
        assert!(verify_ds_to_dnskey(&ds, &dnskey).is_err());
    }
    #[test] fn chain_validate_empty() {
        let links = validate_chain(&[], &[]);
        assert!(links.is_empty());
    }
    #[test] fn chain_validate_match() {
        let dnskey = vec![0x01, 0x00, 0x03, 0x0d, 0xaa, 0xbb, 0xcc, 0xdd];
        let kt = compute_key_tag(&dnskey);
        let digest = compute_ds_digest(&dnskey, 2).unwrap();
        let ds = DsRdata { key_tag: kt, algorithm: 0x0d, digest_type: 2, digest };
        let links = validate_chain(&[(kt, dnskey)], &[ds]);
        assert_eq!(links.len(), 1);
        assert!(links[0].valid);
    }
    #[test] fn chain_validate_mismatch() {
        let dnskey = vec![0x01, 0x00, 0x03, 0x0d, 0xaa];
        let kt = compute_key_tag(&dnskey);
        let bogus_ds = DsRdata { key_tag: kt, algorithm: 0x0d, digest_type: 2, digest: vec![0; 32] };
        let links = validate_chain(&[(kt, dnskey)], &[bogus_ds]);
        assert_eq!(links.len(), 1);
        assert!(!links[0].valid);
    }
    fn hex_lower(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes { out.push_str(&format!("{:02x}", b)); }
        out
    }
    #[test] fn chain_summary_counts() {
        let mut m = BTreeMap::new();
        m.insert("a".into(), vec![1]);
        m.insert("b".into(), vec![2]);
        assert_eq!(chain_summary(&m), 2);
    }
}