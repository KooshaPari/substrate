// TLS record layer (RFC 5246 §6.2) — ContentType + ProtocolVersion + length + fragment.
// Parser returns slice indices into the input buffer; encoder writes to a Vec<u8>.
pub const TLS_HEADER_LEN: usize = 5;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ContentType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}
impl ContentType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            20 => Some(Self::ChangeCipherSpec),
            21 => Some(Self::Alert),
            22 => Some(Self::Handshake),
            23 => Some(Self::ApplicationData),
            _ => None,
        }
    }
}
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}
impl ProtocolVersion {
    pub fn tls_1_2() -> Self { Self { major: 3, minor: 3 } }
    pub fn tls_1_3() -> Self { Self { major: 3, minor: 3 } }
    pub fn to_be_bytes(self) -> [u8; 2] { [self.major, self.minor] }
}
#[derive(Debug, PartialEq)]
pub struct TlsRecord<'a> {
    pub content_type: ContentType,
    pub version: ProtocolVersion,
    pub payload: &'a [u8],
}
#[derive(Debug, PartialEq)]
pub enum TlsError {
    TooShort,
    BadContentType,
    BadLength,
    Truncated,
}
pub fn parse_record(input: &[u8]) -> Result<TlsRecord, TlsError> {
    if input.len() < TLS_HEADER_LEN { return Err(TlsError::TooShort); }
    let content_type = ContentType::from_u8(input[0]).ok_or(TlsError::BadContentType)?;
    let version = ProtocolVersion { major: input[1], minor: input[2] };
    let length = u16::from_be_bytes([input[3], input[4]]) as usize;
    if length > 16384 + 2048 { return Err(TlsError::BadLength); }
    if input.len() < TLS_HEADER_LEN + length { return Err(TlsError::Truncated); }
    Ok(TlsRecord {
        content_type,
        version,
        payload: &input[TLS_HEADER_LEN..TLS_HEADER_LEN + length],
    })
}
pub fn write_record(content_type: ContentType, version: ProtocolVersion, payload: &[u8], out: &mut Vec<u8>) {
    out.push(content_type as u8);
    out.extend_from_slice(&version.to_be_bytes());
    let len = payload.len().min(16384 + 2048) as u16;
    out.push((len >> 8) as u8);
    out.push((len & 0xff) as u8);
    out.extend_from_slice(&payload[..len as usize]);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic_app_data() {
        let mut buf = vec![23, 3, 3, 0, 5];
        buf.extend_from_slice(b"hello");
        let r = parse_record(&buf).unwrap();
        assert_eq!(r.content_type, ContentType::ApplicationData);
        assert_eq!(r.version.major, 3);
        assert_eq!(r.payload, b"hello");
    }
    #[test]
    fn parse_handshake() {
        let mut buf = vec![22, 3, 3, 0, 4];
        buf.extend_from_slice(&[1, 2, 3, 4]);
        let r = parse_record(&buf).unwrap();
        assert_eq!(r.content_type, ContentType::Handshake);
        assert_eq!(r.payload, &[1, 2, 3, 4]);
    }
    #[test]
    fn parse_alert() {
        let buf = vec![21, 3, 3, 0, 2, 1, 0];
        let r = parse_record(&buf).unwrap();
        assert_eq!(r.content_type, ContentType::Alert);
        assert_eq!(r.payload, &[1, 0]);
    }
    #[test]
    fn parse_too_short_header() {
        assert_eq!(parse_record(&[1, 2, 3]), Err(TlsError::TooShort));
    }
    #[test]
    fn parse_bad_content_type() {
        let buf = vec![99, 3, 3, 0, 0];
        assert_eq!(parse_record(&buf), Err(TlsError::BadContentType));
    }
    #[test]
    fn parse_truncated_payload() {
        // claims 10 bytes but only has 5
        let buf = vec![23, 3, 3, 0, 10, 1, 2, 3, 4, 5];
        assert_eq!(parse_record(&buf), Err(TlsError::Truncated));
    }
    #[test]
    fn parse_overlong_length() {
        let buf = vec![23, 3, 3, 0xff, 0xff]; // length = 65535
        assert_eq!(parse_record(&buf), Err(TlsError::BadLength));
    }
    #[test]
    fn parse_exact_boundary() {
        let mut buf = vec![23, 3, 3, 0, 0];
        let r = parse_record(&buf).unwrap();
        assert_eq!(r.payload, &[] as &[u8]);
    }
    #[test]
    fn write_basic() {
        let mut out = Vec::new();
        write_record(ContentType::ApplicationData, ProtocolVersion::tls_1_2(), b"world", &mut out);
        assert_eq!(out, vec![23, 3, 3, 0, 5, b'w', b'o', b'r', b'l', b'd']);
    }
    #[test]
    fn write_then_parse_roundtrip() {
        let mut out = Vec::new();
        let payload = &[1u8, 2, 3, 4, 5, 6, 7, 8];
        write_record(ContentType::Handshake, ProtocolVersion { major: 3, minor: 3 }, payload, &mut out);
        let r = parse_record(&out).unwrap();
        assert_eq!(r.content_type, ContentType::Handshake);
        assert_eq!(r.payload, payload);
    }
    #[test]
    fn content_type_from_u8() {
        assert_eq!(ContentType::from_u8(20), Some(ContentType::ChangeCipherSpec));
        assert_eq!(ContentType::from_u8(23), Some(ContentType::ApplicationData));
        assert_eq!(ContentType::from_u8(0), None);
    }
    #[test]
    fn empty_payload_record() {
        let mut buf = vec![22, 3, 3, 0, 0];
        let r = parse_record(&buf).unwrap();
        assert_eq!(r.payload, &[] as &[u8]);
        assert_eq!(r.content_type, ContentType::Handshake);
    }
}
