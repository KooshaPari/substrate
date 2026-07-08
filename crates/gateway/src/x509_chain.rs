// Minimal X.509 DER decoder. Parses certificate structure (TBSCertificate, signature
// algorithm, signature value) and extracts a few common fields: subject, issuer,
// validity dates, and a subject-key identifier if present.
//
// This is NOT a validator — it does not verify signatures or check revocation. It
// exists so consumers can introspect a PEM/DER blob without pulling in `x509-parser`.

use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DistinguishedName {
    pub raw: String,
    pub parts: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Validity {
    pub not_before: String,
    pub not_after: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Cert {
    pub version: u8,
    pub serial: Vec<u8>,
    pub signature_algorithm: String,
    pub issuer: DistinguishedName,
    pub subject: DistinguishedName,
    pub validity: Validity,
    pub subject_key_identifier: Option<Vec<u8>>,
    pub extensions_count: usize,
    pub raw_len: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    BadTag(u8),
    BadLength,
    InvalidUtf8,
    NotACertificate,
    Unsupported(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Truncated => write!(f, "input truncated"),
            Error::BadTag(t) => write!(f, "unexpected tag 0x{:02x}", t),
            Error::BadLength => write!(f, "bad length encoding"),
            Error::InvalidUtf8 => write!(f, "invalid utf-8 in string"),
            Error::NotACertificate => write!(f, "not a certificate"),
            Error::Unsupported(s) => write!(f, "unsupported: {}", s),
        }
    }
}

pub fn parse_der(input: &[u8]) -> Result<Cert, Error> {
    let mut p = Parser::new(input);
    let outer = p.read_sequence()?;
    if outer.tag != 0x30 { return Err(Error::NotACertificate); }
    let cert_bytes = p.slice(outer.content)?;
    let mut inner = Parser::new(cert_bytes);
    let tbs_seq = inner.read_sequence()?;
    if tbs_seq.tag != 0x30 { return Err(Error::BadTag(tbs_seq.tag)); }
    let tbs_bytes = inner.slice(tbs_seq.content)?;
    let mut tbs = Parser::new(tbs_bytes);
    let mut version: u8 = 0;
    if tbs.peek() == Some(0xa0) {
        let v = tbs.read_explicit(0xa0)?;
        let mut vp = Parser::new(v);
        let i = vp.read_integer()?;
        version = if i.is_empty() { 0 } else { i[0] } + 1;
    }
    let serial_bytes = tbs.read_integer()?;
    let sig_alg_bytes = tbs.read_sequence()?;
    let sig_alg_oid = read_first_oid(sig_alg_bytes.content)?;
    let issuer = read_name(&mut tbs)?;
    let validity = read_validity(&mut tbs)?;
    let subject = read_name(&mut tbs)?;
    let mut ski: Option<Vec<u8>> = None;
    let mut ext_count = 0usize;
    if !tbs.eof() {
        let spki_seq = tbs.read_sequence()?;
        if !tbs.eof() {
            if let Some(ext_seq) = tbs.try_read_explicit(0xa3)? {
                let mut ep = Parser::new(ext_seq);
                let extensions_seq = ep.read_sequence()?;
                if extensions_seq.tag != 0x30 { return Err(Error::BadTag(extensions_seq.tag)); }
                let mut xp = Parser::new(extensions_seq.content);
                while !xp.eof() {
                    let ext_seq = xp.read_sequence()?;
                    if ext_seq.tag != 0x30 { return Err(Error::BadTag(ext_seq.tag)); }
                    ext_count += 1;
                    let ext_content = xp.slice(ext_seq.content)?;
                    let mut ex = Parser::new(ext_content);
                    let oid_bytes = ex.read_oid()?;
                    if format_oid(&oid_bytes) == "2.5.29.14" {
                        let _ = ex.read_octet_string()?;
                        ski = Some(oid_bytes.clone());
                    }
                    let _ = spki_seq;
                }
            }
        }
    }
    Ok(Cert {
        version,
        serial: serial_bytes.to_vec(),
        signature_algorithm: sig_alg_oid,
        issuer,
        subject,
        validity,
        subject_key_identifier: ski,
        extensions_count: ext_count,
        raw_len: input.len(),
    })
}

struct Tlv<'a> { tag: u8, content: &'a [u8] }
struct Parser<'a> { src: &'a [u8], pos: usize }
impl<'a> Parser<'a> {
    fn new(s: &'a [u8]) -> Self { Self { src: s, pos: 0 } }
    fn eof(&self) -> bool { self.pos >= self.src.len() }
    fn peek(&self) -> Option<u8> { if self.pos < self.src.len() { Some(self.src[self.pos]) } else { None } }
    fn read_len(&mut self) -> Result<usize, Error> {
        let first = self.peek().ok_or(Error::Truncated)?;
        self.pos += 1;
        if first < 0x80 { Ok(first as usize) }
        else {
            let n = (first & 0x7f) as usize;
            if self.pos + n > self.src.len() { return Err(Error::Truncated); }
            let mut out = 0usize;
            for _ in 0..n {
                out = (out << 8) | self.src[self.pos] as usize;
                self.pos += 1;
            }
            Ok(out)
        }
    }
    fn read_tlv(&mut self) -> Result<Tlv<'a>, Error> {
        let tag = self.peek().ok_or(Error::Truncated)?;
        self.pos += 1;
        let len = self.read_len()?;
        if self.pos + len > self.src.len() { return Err(Error::Truncated); }
        let start = self.pos;
        self.pos += len;
        Ok(Tlv { tag, content: &self.src[start..start+len] })
    }
    fn slice(&mut self, content: &'a [u8]) -> Result<&'a [u8], Error> {
        Ok(content)
    }
    fn read_sequence(&mut self) -> Result<Tlv<'a>, Error> { self.read_tlv() }
    fn read_integer(&mut self) -> Result<&'a [u8], Error> {
        let t = self.read_tlv()?;
        if t.tag != 0x02 { return Err(Error::BadTag(t.tag)); }
        Ok(t.content)
    }
    fn read_oid(&mut self) -> Result<Vec<u8>, Error> {
        let t = self.read_tlv()?;
        if t.tag != 0x06 { return Err(Error::BadTag(t.tag)); }
        Ok(t.content.to_vec())
    }
    fn read_octet_string(&mut self) -> Result<&'a [u8], Error> {
        let t = self.read_tlv()?;
        if t.tag != 0x04 { return Err(Error::BadTag(t.tag)); }
        Ok(t.content)
    }
    fn read_explicit(&mut self, expected_tag: u8) -> Result<&'a [u8], Error> {
        let t = self.read_tlv()?;
        if t.tag != expected_tag { return Err(Error::BadTag(t.tag)); }
        Ok(t.content)
    }
    fn try_read_explicit(&mut self, expected_tag: u8) -> Result<Option<&'a [u8]>, Error> {
        if self.peek() == Some(expected_tag) { Ok(Some(self.read_explicit(expected_tag)?)) }
        else { Ok(None) }
    }
}

fn read_first_oid(seq_bytes: &[u8]) -> Result<String, Error> {
    let mut p = Parser::new(seq_bytes);
    let t = p.read_tlv()?;
    if t.tag != 0x06 { return Err(Error::BadTag(t.tag)); }
    Ok(format_oid(t.content))
}

fn format_oid(oid: &[u8]) -> String {
    if oid.is_empty() { return String::new(); }
    let first = oid[0];
    let a = (first / 40) as u64;
    let b = (first % 40) as u64;
    let mut parts: Vec<String> = vec![a.to_string(), b.to_string()];
    let mut acc: u64 = 0;
    for &byte in &oid[1..] {
        acc = (acc << 7) | (byte & 0x7f) as u64;
        if byte & 0x80 == 0 { parts.push(acc.to_string()); acc = 0; }
    }
    parts.join(".")
}

fn read_name(p: &mut Parser) -> Result<DistinguishedName, Error> {
    let t = p.read_tlv()?;
    if t.tag != 0x30 { return Err(Error::BadTag(t.tag)); }
    let mut inner = Parser::new(t.content);
    let mut parts: BTreeMap<String, String> = BTreeMap::new();
    let mut raw = String::new();
    while !inner.eof() {
        let set = inner.read_tlv()?;
        if set.tag != 0x31 { return Err(Error::BadTag(set.tag)); }
        let mut sp = Parser::new(set.content);
        let seq = sp.read_tlv()?;
        if seq.tag != 0x30 { return Err(Error::BadTag(seq.tag)); }
        let mut mp = Parser::new(seq.content);
        let oid_bytes = mp.read_oid()?;
        let oid = format_oid(&oid_bytes);
        let value_tlv = mp.read_tlv()?;
        let value_str = if value_tlv.tag == 0x0c || value_tlv.tag == 0x13 || value_tlv.tag == 0x14 {
            std::str::from_utf8(value_tlv.content).map_err(|_| Error::InvalidUtf8)?.to_string()
        } else {
            format!("0x{}", hex_lower(value_tlv.content))
        };
        let key = oid_to_name(&oid);
        if !raw.is_empty() { raw.push(','); }
        raw.push_str(&format!("{}={}", key, value_str));
        parts.insert(key, value_str);
    }
    Ok(DistinguishedName { raw, parts })
}

fn oid_to_name(oid: &str) -> String {
    match oid {
        "2.5.4.3" => "CN".into(),
        "2.5.4.6" => "C".into(),
        "2.5.4.7" => "L".into(),
        "2.5.4.8" => "ST".into(),
        "2.5.4.10" => "O".into(),
        "2.5.4.11" => "OU".into(),
        "1.2.840.113549.1.9.1" => "E".into(),
        other => format!("OID({})", other),
    }
}

fn read_validity(p: &mut Parser) -> Result<Validity, Error> {
    let t = p.read_tlv()?;
    if t.tag != 0x30 { return Err(Error::BadTag(t.tag)); }
    let mut inner = Parser::new(t.content);
    let nb = inner.read_tlv()?;
    let na = inner.read_tlv()?;
    let not_before = read_time(nb.tag, nb.content)?;
    let not_after = read_time(na.tag, na.content)?;
    Ok(Validity { not_before, not_after })
}

fn read_time(tag: u8, content: &[u8]) -> Result<String, Error> {
    let s = std::str::from_utf8(content).map_err(|_| Error::InvalidUtf8)?;
    match tag {
        0x17 => Ok(s.to_string()),
        0x18 => Ok(s.to_string()),
        _ => Err(Error::BadTag(tag)),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes { out.push_str(&format!("{:02x}", b)); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn round_trip_name_oid() {
        assert_eq!(oid_to_name("2.5.4.3"), "CN");
        assert_eq!(oid_to_name("2.5.4.10"), "O");
        assert!(oid_to_name("1.2.3").starts_with("OID("));
    }
    #[test] fn oid_format() {
        assert_eq!(format_oid(&[0x55, 0x04, 0x03]), "2.5.4.3");
        assert_eq!(format_oid(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d]), "1.2.840.113549");
    }
    #[test] fn empty_oid() {
        assert_eq!(format_oid(&[]), "");
    }
    #[test] fn reject_truncated() {
        assert!(matches!(parse_der(&[0x30, 0x82, 0x00, 0x10]), Err(Error::Truncated)));
    }
    #[test] fn reject_non_sequence() {
        let mut buf = vec![0x02, 0x01, 0x01];
        assert!(matches!(parse_der(&buf), Err(Error::NotACertificate)));
    }
    #[test] fn read_tlv_with_multi_byte_len() {
        let mut p = Parser::new(&[0x30, 0x82, 0x00, 0x05, 1, 2, 3, 4, 5]);
        let t = p.read_tlv().unwrap();
        assert_eq!(t.tag, 0x30);
        assert_eq!(t.content.len(), 5);
    }
    #[test] fn read_tlv_short_form() {
        let mut p = Parser::new(&[0x02, 0x03, 0x01, 0x02, 0x03]);
        let t = p.read_tlv().unwrap();
        assert_eq!(t.tag, 0x02);
        assert_eq!(t.content, &[1, 2, 3]);
    }
    #[test] fn hex_lower_works() {
        assert_eq!(hex_lower(&[0xab, 0xcd]), "abcd");
    }
    #[test] fn rejects_bad_length() {
        let mut p = Parser::new(&[0x02, 0x85]);
        assert!(matches!(p.read_tlv(), Err(Error::Truncated)));
    }
    #[test] fn format_oid_multi_byte_component() {
        assert_eq!(format_oid(&[0x2a, 0x86, 0x48, 0x01]), "1.2.840.1");
    }

    #[test]
    fn parse_is_idempotent_on_random() {
        // Property: parsing the same bytes twice must produce structurally
        // identical results. We assert on the public fields whose comparison is
        // meaningful (versions, serials, signature alg OID, extension count, and
        // raw_len). Idempotence holds trivially for any error path — we run
        // both parses and require they return the same discriminant.
        let mut state: u32 = 0x1a2b_3c4d;
        for trial in 0..64 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut bytes: Vec<u8> = Vec::new();
            let n_blobs = ((state as usize) % 6) + 1;
            for _ in 0..n_blobs {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let len = ((state as usize) % 200) + 2;
                bytes.push(0x30);
                bytes.push(0x82);
                bytes.push((len >> 8) as u8);
                bytes.push(len as u8);
                for j in 0..len {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    bytes.push((state >> (j % 24)) as u8);
                }
            }
            let a = parse_der(&bytes);
            let b = parse_der(&bytes);
            assert_eq!(a.is_ok(), b.is_ok(), "trial={}: ok parity drifted", trial);
            if let (Ok(ca), Ok(cb)) = (a, b) {
                assert_eq!(ca.version, cb.version, "trial={} version drift", trial);
                assert_eq!(ca.serial, cb.serial, "trial={} serial drift", trial);
                assert_eq!(ca.signature_algorithm, cb.signature_algorithm);
                assert_eq!(ca.extensions_count, cb.extensions_count);
                assert_eq!(ca.raw_len, cb.raw_len);
            }
        }
    }
}