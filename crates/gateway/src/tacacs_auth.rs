// Minimal TACACS+ packet parser.
//
// References:
//   RFC 8907 - The Terminal Access Controller Access-Control System Plus (TACACS+) Protocol
//   draft-grant-tacacs-02 - the original TACACS+ draft
//   - Packet header is 12 bytes:
//       byte 0:    major_version (high nibble) | minor_version (low nibble)
//       byte 1:    type
//       byte 2:    seq_no
//       byte 3:    flags
//       bytes 4-7: session_id (network byte order)
//       bytes 8-11: length (network byte order; excludes the 12-byte header)
//   - All multi-byte fields are network byte order (big-endian).
//   - Authentication START body (Section 5.1) is:
//       action(1) priv_lvl(1) authen_type(1) authen_service(1) +
//       user_len(1) port_len(1) rem_addr_len(1) data_len(1) +
//       user(user_len) port(port_len) rem_addr(rem_addr_len) data(data_len)
//   - Length-prefixed fields are NOT null-terminated.

/// TACACS+ packet types (RFC 8907 Section 4.3 / draft-grant-tacacs-02).
pub const TAC_PLUS_AUTHEN: u8 = 0x01;
pub const TAC_PLUS_AUTHOR: u8 = 0x02;
pub const TAC_PLUS_ACCT: u8 = 0x03;

/// Flag bits at byte 3 of the header.
pub const TAC_PLUS_UNENCRYPTED_FLAG: u8 = 0x01;
pub const TAC_PLUS_SINGLE_CONNECT_FLAG: u8 = 0x04;

/// Authentication START action values (Section 5.1.1).
pub const TAC_PLUS_AUTHEN_LOGIN: u8 = 0x01;
pub const TAC_PLUS_AUTHEN_CHPASS: u8 = 0x02;
pub const TAC_PLUS_AUTHEN_SENDAUTH: u8 = 0x04;

/// Authentication type values (Section 5.1.2).
pub const TAC_PLUS_AUTHEN_TYPE_ASCII: u8 = 0x01;
pub const TAC_PLUS_AUTHEN_TYPE_PAP: u8 = 0x02;
pub const TAC_PLUS_AUTHEN_TYPE_CHAP: u8 = 0x03;
pub const TAC_PLUS_AUTHEN_TYPE_MSCHAP: u8 = 0x05;
pub const TAC_PLUS_AUTHEN_TYPE_MSCHAPV2: u8 = 0x06;

/// Authentication service values (Section 5.1.3).
pub const TAC_PLUS_AUTHEN_SVC_NONE: u8 = 0x00;
pub const TAC_PLUS_AUTHEN_SVC_LOGIN: u8 = 0x01;
pub const TAC_PLUS_AUTHEN_SVC_ENABLE: u8 = 0x02;
pub const TAC_PLUS_AUTHEN_SVC_PPP: u8 = 0x03;
pub const TAC_PLUS_AUTHEN_SVC_PT: u8 = 0x05;
pub const TAC_PLUS_AUTHEN_SVC_RCMD: u8 = 0x06;
pub const TAC_PLUS_AUTHEN_SVC_X25: u8 = 0x07;
pub const TAC_PLUS_AUTHEN_SVC_NASI: u8 = 0x08;
pub const TAC_PLUS_AUTHEN_SVC_FWPROXY: u8 = 0x09;

/// Outer TACACS+ packet header (12 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct TacacsHeader {
    pub major_version: u8,
    pub minor_version: u8,
    pub pkt_type: u8,
    pub seq_no: u8,
    pub flags: u8,
    pub session_id: u32,
    pub length: u32,
}

/// Authentication START body (Section 5.1).
#[derive(Debug, Clone, PartialEq)]
pub struct AuthenStart {
    pub action: u8,
    pub priv_lvl: u8,
    pub authen_type: u8,
    pub authen_service: u8,
    pub user: String,
    pub port: String,
    pub rem_addr: String,
    pub data: String,
}

#[derive(Debug, PartialEq)]
pub enum TacacsError {
    TooShort {
        needed: usize,
        have: usize,
    },
    BadVersion {
        major: u8,
        minor: u8,
    },
    LengthMismatch {
        field: &'static str,
        declared: u8,
        available: usize,
    },
}

/// Parse the 12-byte TACACS+ outer header.
pub fn parse_header(buf: &[u8]) -> Result<TacacsHeader, TacacsError> {
    if buf.len() < 12 {
        return Err(TacacsError::TooShort {
            needed: 12,
            have: buf.len(),
        });
    }
    // Byte 0: high nibble = major, low nibble = minor.
    let major_version = (buf[0] >> 4) & 0x0f;
    let minor_version = buf[0] & 0x0f;
    if major_version != 0xc {
        return Err(TacacsError::BadVersion {
            major: major_version,
            minor: minor_version,
        });
    }
    let session_id = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let length = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
    Ok(TacacsHeader {
        major_version,
        minor_version,
        pkt_type: buf[1],
        seq_no: buf[2],
        flags: buf[3],
        session_id,
        length,
    })
}

/// Parse an Authentication START body (Section 5.1 of RFC 8907).
pub fn parse_authen_start(body: &[u8]) -> Result<AuthenStart, TacacsError> {
    // Need at least 8 bytes of fixed header (4 enum bytes + 4 length bytes).
    if body.len() < 8 {
        return Err(TacacsError::TooShort {
            needed: 8,
            have: body.len(),
        });
    }
    let action = body[0];
    let priv_lvl = body[1];
    let authen_type = body[2];
    let authen_service = body[3];
    let user_len = body[4];
    let port_len = body[5];
    let rem_addr_len = body[6];
    let data_len = body[7];

    let needed = 8
        + (user_len as usize)
        + (port_len as usize)
        + (rem_addr_len as usize)
        + (data_len as usize);
    if body.len() < needed {
        return Err(TacacsError::TooShort {
            needed,
            have: body.len(),
        });
    }

    let mut cursor = 8usize;
    let user = read_field(body, &mut cursor, user_len, "user")?;
    let port = read_field(body, &mut cursor, port_len, "port")?;
    let rem_addr = read_field(body, &mut cursor, rem_addr_len, "rem_addr")?;
    let data = read_field(body, &mut cursor, data_len, "data")?;

    Ok(AuthenStart {
        action,
        priv_lvl,
        authen_type,
        authen_service,
        user,
        port,
        rem_addr,
        data,
    })
}

/// Build a TACACS+ outer header (12 bytes). Useful for round-trip and header-only tests.
pub fn build_header(h: &TacacsHeader) -> [u8; 12] {
    let mut out = [0u8; 12];
    // Byte 0: high nibble = major, low nibble = minor.
    out[0] = (h.major_version << 4) | (h.minor_version & 0x0f);
    out[1] = h.pkt_type;
    out[2] = h.seq_no;
    out[3] = h.flags;
    let sid = h.session_id.to_be_bytes();
    out[4] = sid[0];
    out[5] = sid[1];
    out[6] = sid[2];
    out[7] = sid[3];
    let len = h.length.to_be_bytes();
    out[8] = len[0];
    out[9] = len[1];
    out[10] = len[2];
    out[11] = len[3];
    out
}

fn read_field(
    body: &[u8],
    cursor: &mut usize,
    len: u8,
    name: &'static str,
) -> Result<String, TacacsError> {
    let end = *cursor + len as usize;
    if end > body.len() {
        return Err(TacacsError::LengthMismatch {
            field: name,
            declared: len,
            available: body.len().saturating_sub(*cursor),
        });
    }
    // RFC 8907: "The lengths of data and message fields in a packet are specified by their
    // corresponding length field (and are not null terminated)." UTF-8 lossy decode keeps the
    // parser tolerant of binary protocol fields without dropping bytes.
    let slice = &body[*cursor..end];
    let s = String::from_utf8_lossy(slice).into_owned();
    *cursor = end;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a valid Authentication START body and return the bytes plus the expected
    /// `AuthenStart` struct. Bytes follow RFC 8907 Section 5.1.
    fn sample_authen_body() -> (Vec<u8>, AuthenStart) {
        let user = b"alice";
        let port = b"ttyS0";
        let rem = b"10.0.0.1";
        let data = b""; // empty data field is common for ASCII login prompt

        let mut body = Vec::new();
        body.push(TAC_PLUS_AUTHEN_LOGIN); // action
        body.push(0x00); // priv_lvl
        body.push(TAC_PLUS_AUTHEN_TYPE_ASCII); // authen_type
        body.push(TAC_PLUS_AUTHEN_SVC_LOGIN); // authen_service
        body.push(user.len() as u8);
        body.push(port.len() as u8);
        body.push(rem.len() as u8);
        body.push(data.len() as u8);
        body.extend_from_slice(user);
        body.extend_from_slice(port);
        body.extend_from_slice(rem);
        body.extend_from_slice(data);

        let expected = AuthenStart {
            action: TAC_PLUS_AUTHEN_LOGIN,
            priv_lvl: 0,
            authen_type: TAC_PLUS_AUTHEN_TYPE_ASCII,
            authen_service: TAC_PLUS_AUTHEN_SVC_LOGIN,
            user: "alice".to_string(),
            port: "ttyS0".to_string(),
            rem_addr: "10.0.0.1".to_string(),
            data: "".to_string(),
        };
        (body, expected)
    }

    #[test]
    fn header_minimum_bytes() {
        let err = parse_header(&[0u8; 11]).unwrap_err();
        assert_eq!(
            err,
            TacacsError::TooShort {
                needed: 12,
                have: 11
            }
        );
    }

    #[test]
    fn header_rejects_non_0xc_major() {
        // Byte 0 = 0x42 -> major = 4, minor = 2 (not 0xc).
        let buf = [0x42, TAC_PLUS_AUTHEN, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let err = parse_header(&buf).unwrap_err();
        assert_eq!(err, TacacsError::BadVersion { major: 4, minor: 2 });
    }

    #[test]
    fn header_round_trip() {
        let h = TacacsHeader {
            major_version: 0xc,
            minor_version: 0x1,
            pkt_type: TAC_PLUS_AUTHEN,
            seq_no: 0x01,
            flags: TAC_PLUS_UNENCRYPTED_FLAG,
            session_id: 0x11223344,
            length: 0x00000020,
        };
        let bytes = build_header(&h);
        // Byte 0 packs major=0xc (high nibble) and minor=0x1 (low nibble) => 0xc1.
        assert_eq!(bytes[0], 0xc1);
        assert_eq!(bytes[1], TAC_PLUS_AUTHEN);
        assert_eq!(bytes[2], 0x01);
        assert_eq!(bytes[3], TAC_PLUS_UNENCRYPTED_FLAG);
        assert_eq!(&bytes[4..8], &[0x11, 0x22, 0x33, 0x44]);
        assert_eq!(&bytes[8..12], &[0x00, 0x00, 0x00, 0x20]);
        let parsed = parse_header(&bytes).unwrap();
        assert_eq!(parsed.major_version, 0xc);
        assert_eq!(parsed.minor_version, 0x1);
        assert_eq!(parsed.session_id, 0x11223344);
        assert_eq!(parsed.length, 0x20);
        assert_eq!(parsed.flags, TAC_PLUS_UNENCRYPTED_FLAG);
    }

    #[test]
    fn header_single_connect_flag_at_byte3() {
        // Confirm byte 3 is flags (NOT seq_no).
        let mut bytes = [0u8; 12];
        bytes[0] = 0xc0; // major=0xc, minor=0
        bytes[1] = TAC_PLUS_AUTHEN;
        bytes[2] = 0x05; // seq_no
        bytes[3] = TAC_PLUS_SINGLE_CONNECT_FLAG; // flags
        bytes[4..8].copy_from_slice(&0xdeadbeef_u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&0x10_u32.to_be_bytes());
        let h = parse_header(&bytes).unwrap();
        assert_eq!(h.seq_no, 0x05);
        assert_eq!(h.flags, TAC_PLUS_SINGLE_CONNECT_FLAG);
        assert_eq!(h.session_id, 0xdeadbeef);
        assert_eq!(h.length, 0x10);
    }

    #[test]
    fn authen_start_parses_canonical_login() {
        let (body, expected) = sample_authen_body();
        let parsed = parse_authen_start(&body).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn authen_start_short_body() {
        // Only 7 bytes -- not enough for the 8-byte fixed header.
        let err = parse_authen_start(&[0u8; 7]).unwrap_err();
        assert_eq!(err, TacacsError::TooShort { needed: 8, have: 7 });
    }

    #[test]
    fn authen_start_truncated_variable_field() {
        // header says user_len=5 but body only has 3 bytes after the 8-byte fixed header.
        let body = vec![
            TAC_PLUS_AUTHEN_LOGIN,
            0,
            TAC_PLUS_AUTHEN_TYPE_ASCII,
            TAC_PLUS_AUTHEN_SVC_LOGIN,
            5, // user_len
            0,
            0,
            0,
            b'a',
            b'b',
            b'c',
        ];
        let err = parse_authen_start(&body).unwrap_err();
        assert!(matches!(err, TacacsError::TooShort { .. }));
    }

    #[test]
    fn authen_start_empty_data_field() {
        // data_len = 0 must be allowed (e.g. initial ASCII login prompt has no data).
        let (body, expected) = sample_authen_body();
        // Sanity: declared total = 8 + 5 + 5 + 8 + 0 = 26.
        assert_eq!(body.len(), 26);
        let parsed = parse_authen_start(&body).unwrap();
        assert_eq!(parsed.data, "");
        assert_eq!(parsed, expected);
    }

    #[test]
    fn authen_start_pap_with_data() {
        // PAP carries the password inside the data field.
        let user = b"bob";
        let port = b"";
        let rem = b"";
        let data = b"hunter2";

        let mut body = Vec::new();
        body.push(TAC_PLUS_AUTHEN_LOGIN);
        body.push(0x00);
        body.push(TAC_PLUS_AUTHEN_TYPE_PAP);
        body.push(TAC_PLUS_AUTHEN_SVC_PPP);
        body.push(user.len() as u8);
        body.push(port.len() as u8);
        body.push(rem.len() as u8);
        body.push(data.len() as u8);
        body.extend_from_slice(user);
        body.extend_from_slice(port);
        body.extend_from_slice(rem);
        body.extend_from_slice(data);

        let parsed = parse_authen_start(&body).unwrap();
        assert_eq!(parsed.authen_type, TAC_PLUS_AUTHEN_TYPE_PAP);
        assert_eq!(parsed.authen_service, TAC_PLUS_AUTHEN_SVC_PPP);
        assert_eq!(parsed.user, "bob");
        assert_eq!(parsed.data, "hunter2");
        assert_eq!(parsed.port, "");
        assert_eq!(parsed.rem_addr, "");
    }
}
