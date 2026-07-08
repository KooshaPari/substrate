// Minimal RFC 1035 DNS message parser (header + question + simple record decoding)
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qd_count: u16,
    pub an_count: u16,
    pub ns_count: u16,
    pub ar_count: u16,
}
pub struct DnsQuestion {
    pub qname: String,
    pub qtype: u16,
    pub qclass: u16,
}
pub struct DnsRecord {
    pub name: String,
    pub rtype: u16,
    pub rclass: u16,
    pub ttl: u32,
    pub rdata: Vec<u8>,
}
#[derive(Debug)]
pub enum DnsParseError {
    TooShort,
    BadLabel,
    BadName,
}
pub fn parse_header(buf: &[u8]) -> Result<DnsHeader, DnsParseError> {
    if buf.len() < 12 { return Err(DnsParseError::TooShort); }
    Ok(DnsHeader {
        id: u16::from_be_bytes([buf[0], buf[1]]),
        flags: u16::from_be_bytes([buf[2], buf[3]]),
        qd_count: u16::from_be_bytes([buf[4], buf[5]]),
        an_count: u16::from_be_bytes([buf[6], buf[7]]),
        ns_count: u16::from_be_bytes([buf[8], buf[9]]),
        ar_count: u16::from_be_bytes([buf[10], buf[11]]),
    })
}
pub fn parse_name(buf: &[u8], mut offset: usize) -> Result<(String, usize), DnsParseError> {
    let mut labels: Vec<String> = Vec::new();
    let mut jumped = false;
    let mut return_offset = offset;
    let mut safety = 0u32;
    loop {
        if offset >= buf.len() { return Err(DnsParseError::BadName); }
        let len = buf[offset];
        if safety > 256 { return Err(DnsParseError::BadName); }
        safety += 1;
        if len == 0 {
            if !jumped { return_offset = offset + 1; }
            break;
        }
        if (len & 0xc0) == 0xc0 {
            if offset + 1 >= buf.len() { return Err(DnsParseError::BadName); }
            if !jumped {
                return_offset = offset + 2;
            }
            let ptr = (((len & 0x3f) as usize) << 8) | (buf[offset + 1] as usize);
            offset = ptr;
            jumped = true;
        } else if (len & 0xc0) == 0 {
            let label_len = len as usize;
            if offset + 1 + label_len > buf.len() { return Err(DnsParseError::BadLabel); }
            let label = std::str::from_utf8(&buf[offset + 1..offset + 1 + label_len])
                .map_err(|_| DnsParseError::BadLabel)?
                .to_string();
            labels.push(label);
            offset += 1 + label_len;
        } else {
            return Err(DnsParseError::BadLabel);
        }
    }
    Ok((labels.join("."), return_offset))
}
pub fn parse_question(buf: &[u8], mut offset: usize) -> Result<(DnsQuestion, usize), DnsParseError> {
    let (qname, after_name) = parse_name(buf, offset)?;
    offset = after_name;
    if offset + 4 > buf.len() { return Err(DnsParseError::TooShort); }
    let qtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
    let qclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
    Ok((DnsQuestion { qname, qtype, qclass }, offset + 4))
}
pub fn parse_record(buf: &[u8], mut offset: usize) -> Result<(DnsRecord, usize), DnsParseError> {
    let (name, after_name) = parse_name(buf, offset)?;
    offset = after_name;
    if offset + 10 > buf.len() { return Err(DnsParseError::TooShort); }
    let rtype = u16::from_be_bytes([buf[offset], buf[offset + 1]]);
    let rclass = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]);
    let ttl = u32::from_be_bytes([buf[offset + 4], buf[offset + 5], buf[offset + 6], buf[offset + 7]]);
    let rdlen = u16::from_be_bytes([buf[offset + 8], buf[offset + 9]]) as usize;
    offset += 10;
    if offset + rdlen > buf.len() { return Err(DnsParseError::TooShort); }
    let rdata = buf[offset..offset + rdlen].to_vec();
    Ok((DnsRecord { name, rtype, rclass, ttl, rdata }, offset + rdlen))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn query_example() -> Vec<u8> {
        // Header: id=0x1234, flags=0x0100 (RD), qd=1
        // Question: example.com, type A (1), class IN (1)
        let mut b = Vec::new();
        b.extend_from_slice(&[0x12, 0x34]);
        b.extend_from_slice(&[0x01, 0x00]);
        b.extend_from_slice(&[0x00, 0x01]);
        b.extend_from_slice(&[0x00, 0x00]);
        b.extend_from_slice(&[0x00, 0x00]);
        b.extend_from_slice(&[0x00, 0x00]);
        b.push(7); b.extend_from_slice(b"example");
        b.push(3); b.extend_from_slice(b"com");
        b.push(0);
        b.extend_from_slice(&[0x00, 0x01]);
        b.extend_from_slice(&[0x00, 0x01]);
        b
    }
    #[test] fn parse_header_basic() {
        let buf = query_example();
        let h = parse_header(&buf).unwrap();
        assert_eq!(h.id, 0x1234);
        assert_eq!(h.qd_count, 1);
        assert_eq!(h.an_count, 0);
    }
    #[test] fn parse_question_basic() {
        let buf = query_example();
        let (q, _) = parse_question(&buf, 12).unwrap();
        assert_eq!(q.qname, "example.com");
        assert_eq!(q.qtype, 1);
        assert_eq!(q.qclass, 1);
    }
    #[test] fn parse_too_short() {
        assert!(parse_header(&[0u8; 5]).is_err());
    }
    #[test] fn parse_name_simple() {
        let buf = vec![3, b'f', b'o', b'o', 3, b'b', b'a', b'r', 0];
        let (name, end) = parse_name(&buf, 0).unwrap();
        assert_eq!(name, "foo.bar");
        assert_eq!(end, 9);
    }
    #[test] fn parse_name_root() {
        let buf = vec![0];
        let (name, end) = parse_name(&buf, 0).unwrap();
        assert_eq!(name, "");
        assert_eq!(end, 1);
    }
    #[test] fn parse_name_compression() {
        // Pointer at offset 10 pointing to offset 0: name "foo"
        let mut buf = vec![0; 16];
        buf[0] = 3; buf[1] = b'f'; buf[2] = b'o'; buf[3] = b'o'; buf[4] = 0;
        buf[10] = 0xc0; buf[11] = 0x00;
        let (name, end) = parse_name(&buf, 10).unwrap();
        assert_eq!(name, "foo");
        assert_eq!(end, 12);
    }
    #[test] fn parse_record_basic() {
        // Build a fake record section
        let mut buf = vec![3, b'b', b'a', b'r', 0, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 60, 0, 4, 1, 2, 3, 4];
        let (r, end) = parse_record(&buf, 0).unwrap();
        assert_eq!(r.name, "bar");
        assert_eq!(r.rtype, 1);
        assert_eq!(r.rclass, 1);
        assert_eq!(r.ttl, 60);
        assert_eq!(r.rdata, vec![1, 2, 3, 4]);
        assert_eq!(end, 19);
    }
    #[test] fn parse_record_too_short() {
        let buf = vec![0; 5];
        assert!(parse_record(&buf, 0).is_err());
    }
    #[test] fn parse_header_flag_qr() {
        let mut buf = vec![0; 12];
        buf[2] = 0x80; // QR bit set
        let h = parse_header(&buf).unwrap();
        assert_eq!(h.flags & 0x8000, 0x8000);
    }
    #[test] fn parse_name_bad_label_length() {
        // label length > remaining
        let buf = vec![10, b'a', b'b', 0];
        assert!(parse_name(&buf, 0).is_err());
    }
}
