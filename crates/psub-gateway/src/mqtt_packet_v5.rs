// Minimal MQTT v5 packet codec. Encodes and decodes the fixed header +
// variable header for the packets the gateway actually needs to proxy
// (Connect, ConnAck, Publish QoS 0, Disconnect). Variable header
// properties are not parsed beyond the variable-length "Property Length"
// prefix — callers receive them as raw bytes and are responsible for
// interpreting individual Property IDs (see MQTT v5 §3.4).
//
// MQTT v5 control packet types:
//   1 = CONNECT     2 = CONNACK     3 = PUBLISH
//   4 = PUBACK      5 = PUBREC      6 = PUBREL
//   7 = PUBCOMP     8 = SUBSCRIBE   9 = SUBACK
//  10 = UNSUBSCRIBE 11 = UNSUBACK   12 = PINGREQ
//  13 = PINGRESP    14 = DISCONNECT  15 = AUTH
//
// Reference: MQTT v5.0 specification, OASIS Standard.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    Connect = 1,
    ConnAck = 2,
    Publish = 3,
    Disconnect = 14,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttError {
    /// The fixed header's "Remaining Length" is malformed (more than 4
    /// continuation bytes).
    BadRemainingLen,
    /// The buffer is shorter than the declared packet length.
    Truncated,
    /// A control packet has an unknown or unsupported type.
    BadType(u8),
    /// The protocol name is not "MQTT" or the level byte is not 5.
    BadProtocol,
}

/// Encode a variable-length "Remaining Length" (1..=4 bytes) and return
/// the bytes. Caller is responsible for prepending the fixed header byte.
pub fn encode_remaining_len(len: u32) -> Result<Vec<u8>, MqttError> {
    if len > 0x0fff_ffff { return Err(MqttError::BadRemainingLen); }
    let mut out = Vec::with_capacity(4);
    let mut x = len;
    loop {
        let mut byte = (x & 0x7f) as u8;
        x >>= 7;
        if x > 0 { byte |= 0x80; }
        out.push(byte);
        if x == 0 { break; }
    }
    Ok(out)
}

/// Decode a variable-length "Remaining Length" from the start of `buf`.
/// Returns `(value, bytes_consumed)`.
pub fn decode_remaining_len(buf: &[u8]) -> Result<(u32, usize), MqttError> {
    let mut multiplier: u32 = 1;
    let mut value: u32 = 0;
    let mut i = 0;
    loop {
        if i >= buf.len() { return Err(MqttError::Truncated); }
        if i >= 4 { return Err(MqttError::BadRemainingLen); }
        let b = buf[i];
        value = value.checked_add((b & 0x7f) as u32 * multiplier)
            .ok_or(MqttError::BadRemainingLen)?;
        i += 1;
        if b & 0x80 == 0 { break; }
        multiplier = multiplier.checked_mul(128).ok_or(MqttError::BadRemainingLen)?;
    }
    Ok((value, i))
}

/// Build a CONNECT packet (no Will, no Username/Password, MQTT v5, no
/// properties). `client_id` must be 1..=23 bytes per the spec.
pub fn encode_connect(client_id: &str) -> Result<Vec<u8>, MqttError> {
    if client_id.is_empty() || client_id.len() > 23 {
        return Err(MqttError::BadProtocol);
    }
    // Variable header: protocol name "MQTT" (4 bytes) + length prefix (2) + level (1) +
    // connect flags (1) + keepalive (2) + properties length (var-int, 0 here).
    let mut vh: Vec<u8> = Vec::new();
    vh.extend_from_slice(&[0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05]);
    vh.push(0x02); // connect flags: clean session only
    vh.extend_from_slice(&0u16.to_be_bytes());
    vh.extend_from_slice(&encode_remaining_len(0)?); // empty properties
    // Payload: client_id (2-byte length prefix + bytes).
    vh.extend_from_slice(&(client_id.len() as u16).to_be_bytes());
    vh.extend_from_slice(client_id.as_bytes());
    let mut out = vec![(PacketType::Connect as u8) << 4];
    out.extend(encode_remaining_len(vh.len() as u32)?);
    out.extend(vh);
    Ok(out)
}

/// Parse a CONNACK packet. Returns the reason code and the raw
/// `Session Present` flag from byte 1.
pub fn decode_connack(buf: &[u8]) -> Result<(u8, bool), MqttError> {
    if buf.len() < 4 { return Err(MqttError::Truncated); }
    if buf[0] >> 4 != PacketType::ConnAck as u8 { return Err(MqttError::BadType(buf[0] >> 4)); }
    let (remlen, used) = decode_remaining_len(&buf[1..])?;
    if buf.len() < 1 + used + remlen as usize { return Err(MqttError::Truncated); }
    let body = &buf[1 + used..1 + used + remlen as usize];
    if body.len() < 2 { return Err(MqttError::Truncated); }
    Ok((body[1], body[0] & 0x01 != 0))
}

/// Build a PUBLISH packet (QoS 0, no properties, no packet id).
pub fn encode_publish_qos0(topic: &str, payload: &[u8]) -> Result<Vec<u8>, MqttError> {
    if topic.is_empty() { return Err(MqttError::BadProtocol); }
    let mut vh: Vec<u8> = Vec::new();
    vh.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    vh.extend_from_slice(topic.as_bytes());
    vh.extend_from_slice(&encode_remaining_len(0)?); // empty properties
    vh.extend_from_slice(payload);
    // QoS 0: fixed-header byte = (3 << 4) | 0 = 0x30. No DUP, no RETAIN.
    let mut out = vec![0x30];
    out.extend(encode_remaining_len(vh.len() as u32)?);
    out.extend(vh);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn encode_decode_remaining_len_round_trip() {
        for &n in &[0u32, 127, 128, 16383, 16384, 0x0fff_ffff] {
            let enc = encode_remaining_len(n).unwrap();
            let (v, used) = decode_remaining_len(&enc).unwrap();
            assert_eq!(v, n);
            assert_eq!(used, enc.len());
        }
    }
    #[test] fn rejects_oversize_remaining_len() {
        assert!(encode_remaining_len(0x1000_0000).is_err());
    }
    #[test] fn rejects_overlong_varint() {
        // 5 continuation bytes → BadRemainingLen.
        let buf = vec![0x80, 0x80, 0x80, 0x80, 0x01];
        assert_eq!(decode_remaining_len(&buf).err(), Some(MqttError::BadRemainingLen));
    }
    #[test] fn connect_minimal_packet() {
        let p = encode_connect("a").unwrap();
        // p[0] = fixed-header byte = 0x10 (CONNECT).
        // p[1] = remaining-length varint = 14.
        // p[2..=3] = Protocol Name length prefix = 0x00, 0x04.
        // p[4..=7] = Protocol Name "MQTT".
        // p[8]    = Protocol Level = 0x05.
        assert_eq!(p[0] >> 4, PacketType::Connect as u8);
        assert_eq!(p[2..=3], [0x00, 0x04]); // Protocol Name length prefix
        assert_eq!(p[4..=7], [b'M', b'Q', b'T', b'T']);
        assert_eq!(p[8], 0x05); // MQTT v5
    }
    #[test] fn publish_qos0_layout() {
        let p = encode_publish_qos0("t", b"hi").unwrap();
        assert_eq!(p[0], 0x30); // PUBLISH, QoS 0
        // topic length prefix (2) + 't' (1) + properties length (1) + payload (2) = 6
        let (rem, used) = decode_remaining_len(&p[1..]).unwrap();
        assert_eq!(rem, 6);
        assert_eq!(used, 1);
        assert_eq!(p.len(), 2 + rem as usize);
    }
    #[test] fn connack_decode() {
        // CONNACK fixed header byte = 0x20, remaining length = 2, body = [0x00, 0x00].
        let p = [0x20, 0x02, 0x00, 0x00];
        let (rc, sp) = decode_connack(&p).unwrap();
        assert_eq!(rc, 0x00);
        assert_eq!(sp, false);
    }
    #[test] fn connack_decode_session_present() {
        let p = [0x20, 0x02, 0x01, 0x00];
        let (_rc, sp) = decode_connack(&p).unwrap();
        assert!(sp);
    }
    #[test] fn empty_client_id_rejected() {
        assert!(encode_connect("").is_err());
    }
}
