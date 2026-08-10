// Minimal Cisco Discovery Protocol (CDP) TLV codec with Meraki OUI
// classification helpers. CDP frames are framed inside IEEE 802.3 +
// LLC/SNAP with a 4-byte CDP header (per Cisco's CDP v2 specification):
//
//     0                   1                   2                   3
//     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//    |   Version (1) |     TTL       |            Checksum           |
//    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//    |          Type (2 bytes BE)    |         Length (2 bytes BE)   |
//    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//    |         Value (Length bytes)                                  |
//    ~                                                               ~
//    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//
// Common TLV type IDs (Cisco, all big-endian on the wire):
//     0x0001 Device ID
//     0x0002 Address
//     0x0003 Port ID
//     0x0004 Capabilities
//     0x0005 Software Version
//     0x0006 Platform
//     0x0007 IP Prefix (CDPv2)
//     0x0008 VTP Management Domain (CDPv2)
//     0x0009 Native VLAN (CDPv2)
//     0x000A Duplex (CDPv2)
//     0x000B Appliance VLAN (CDPv2)
//
// Meraki-specific fingerprints (Cisco OEM):
//     - OUI registered to Meraki (Cisco) is 00:1F:12 (hex bytes
//       0x00, 0x1F, 0x12). A CDP source frame whose source MAC
//       begins with these bytes is a Meraki device.
//     - Meraki `Platform` TLVs start with "Meraki <model>" where
//       model is one of MX / MR / MS / Z / MV (security appliances,
//       wireless APs, switches, teleworker, cameras).
//     - Meraki `Device ID` TLVs are typically "<model><serial>" with
//       no separator (e.g. "MX841234ABCD", "MR42ABCD1234").
//
// This module exposes:
//     - `CdpTlv` struct + `parse_tlvs` for the structural codec;
//     - `parse` (the task-spec entry point) which skips the CDP
//       frame header and returns the TLV list;
//     - `is_meraki_platform` and `is_meraki_device_id` helpers for
//       Meraki OUI / model classification.
//
// We do NOT validate the CDP checksum — CDP frames received from the
// wire must be validated by a higher-level dispatcher; this codec
// only decodes the structural fields.

/// IEEE OUI registered to Meraki (Cisco): 00:1F:12 (three bytes).
pub const MERAKI_OUI: [u8; 3] = [0x00, 0x1F, 0x12];

/// Standard CDP TLV type IDs (defined by Cisco's CDP v2 specification).
pub const TLV_DEVICE_ID: u16 = 0x0001;
pub const TLV_ADDRESS: u16 = 0x0002;
pub const TLV_PORT_ID: u16 = 0x0003;
pub const TLV_CAPABILITIES: u16 = 0x0004;
pub const TLV_SOFTWARE_VERSION: u16 = 0x0005;
pub const TLV_PLATFORM: u16 = 0x0006;
pub const TLV_IP_PREFIX: u16 = 0x0007;
pub const TLV_VTP_MGMT_DOMAIN: u16 = 0x0008;
pub const TLV_NATIVE_VLAN: u16 = 0x0009;
pub const TLV_DUPLEX: u16 = 0x000A;
pub const TLV_APPLIANCE_VLAN: u16 = 0x000B;

/// A single CDP Type-Length-Value record. `value` is returned as
/// bytes (decoding is the caller's responsibility — Device ID,
/// Platform, etc. are ASCII strings; Capabilities is a u32; Address
/// is a structured TLV inside this TLV).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CdpTlv {
    pub type_id: u16,
    pub value: Vec<u8>,
}

/// Parse a CDP frame's TLV list, skipping the 4-byte CDP frame header
/// (Version + TTL + Checksum). This is the task-spec entry point
/// and returns the TLV list directly.
pub fn parse(input: &[u8]) -> Result<Vec<CdpTlv>, String> {
    if input.len() < 4 {
        return Err(format!(
            "CDP frame too short: need at least 4 bytes for header, got {}",
            input.len()
        ));
    }
    let body = &input[4..];
    parse_tlvs(body)
}

/// Parse only the TLV region (assumes the caller has already removed
/// the 4-byte CDP header).
pub fn parse_tlvs(input: &[u8]) -> Result<Vec<CdpTlv>, String> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < input.len() {
        if cursor + 4 > input.len() {
            return Err(format!(
                "CDP TLV truncated at offset {cursor}: need 4-byte header, got {}",
                input.len() - cursor
            ));
        }
        let type_id = u16::from_be_bytes([input[cursor], input[cursor + 1]]);
        let length = u16::from_be_bytes([input[cursor + 2], input[cursor + 3]]);
        let value_end = cursor + 4 + usize::from(length);
        if value_end > input.len() {
            return Err(format!(
                "CDP TLV type=0x{type_id:04x} length={length} exceeds remaining {} bytes",
                input.len() - cursor - 4
            ));
        }
        out.push(CdpTlv {
            type_id,
            value: input[cursor + 4..value_end].to_vec(),
        });
        cursor = value_end;
    }
    Ok(out)
}

/// Return true iff `mac_src` (six-byte source MAC address) starts with
/// the Meraki-registered OUI `00:1F:12`.
pub fn is_meraki_mac(mac_src: &[u8]) -> bool {
    mac_src.len() >= 3 && mac_src[0..3] == MERAKI_OUI
}

/// Return true iff `platform` TLV value starts with "Meraki " and the
/// remainder is one of the canonical Meraki model prefixes (MX, MR,
/// MS, Z, MV). Per Meraki's own CDP implementation, the Platform
/// string is always of the form "Meraki <model> <variant>".
pub fn is_meraki_platform(platform_tlv_value: &[u8]) -> bool {
    if platform_tlv_value.len() < 7 {
        return false;
    }
    if &platform_tlv_value[..7] != b"Meraki " {
        return false;
    }
    let rest = &platform_tlv_value[7..];
    starts_with_any(rest, &[b"MX", b"MR", b"MS", b"Z3", b"Z1", b"MV"])
}

/// Return true iff `device_id` TLV value matches the Meraki
/// `<model><serial>` pattern. The model prefix is the same set as in
/// `is_meraki_platform`.
pub fn is_meraki_device_id(device_id_tlv_value: &[u8]) -> bool {
    if device_id_tlv_value.len() < 4 {
        return false;
    }
    starts_with_any(
        device_id_tlv_value,
        &[b"MX", b"MR", b"MS", b"Z3", b"Z1", b"MV"],
    )
}

fn starts_with_any(input: &[u8], prefixes: &[&[u8]]) -> bool {
    prefixes
        .iter()
        .any(|p| input.len() >= p.len() && &input[..p.len()] == *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference fixture generated by an external Python script
    /// that encodes a CDP frame per Cisco's CDP v2 spec:
    ///
    ///     Version = 0x02
    ///     TTL = 180
    ///     Checksum = 0x0000 (zeroed; not validated by this parser)
    ///     TLV 0x0001 Device ID = b"MX84-1234ABCD" (length 13)
    ///     TLV 0x0006 Platform  = b"Meraki MX84"   (length 11)
    ///     TLV 0x0003 Port ID   = b"Port 5"        (length 6)
    ///
    /// 46 total bytes; payload is 3 TLVs.
    fn cdp_fixture() -> Vec<u8> {
        vec![
            0x02, 0xb4, // Version, TTL
            0x00, 0x00, // Checksum (zeroed)
            0x00, 0x01, 0x00, 0x0d, // TLV 0x0001 len 13
            b'M', b'X', b'8', b'4', b'-', b'1', b'2', b'3', b'4', b'A', b'B', b'C', b'D', 0x00,
            0x06, 0x00, 0x0b, // TLV 0x0006 len 11
            b'M', b'e', b'r', b'a', b'k', b'i', b' ', b'M', b'X', b'8', b'4', 0x00, 0x03, 0x00,
            0x06, // TLV 0x0003 len 6
            b'P', b'o', b'r', b't', b' ', b'5',
        ]
    }

    /// Parse the CDP fixture and verify the structural decode
    /// matches the reference encoding.
    #[test]
    fn parse_fixture() {
        let bytes = cdp_fixture();
        let tlvs = parse(&bytes).expect("parse should succeed");
        assert_eq!(tlvs.len(), 3);
        assert_eq!(tlvs[0].type_id, TLV_DEVICE_ID);
        assert_eq!(tlvs[0].value, b"MX84-1234ABCD");
        assert_eq!(tlvs[1].type_id, TLV_PLATFORM);
        assert_eq!(tlvs[1].value, b"Meraki MX84");
        assert_eq!(tlvs[2].type_id, TLV_PORT_ID);
        assert_eq!(tlvs[2].value, b"Port 5");
    }

    /// Meraki OUI check. Frames whose source MAC begins with
    /// `00:1F:12` are Meraki devices.
    #[test]
    fn meraki_oui_check() {
        let meraki = [0x00, 0x1F, 0x12, 0xAB, 0xCD, 0xEF];
        let cisco = [0x00, 0x40, 0x96, 0x00, 0x00, 0x01];
        assert!(is_meraki_mac(&meraki));
        assert!(!is_meraki_mac(&cisco));
        // Short input must return false (no false-positive).
        assert!(!is_meraki_mac(&[0x00, 0x1F]));
    }

    /// Meraki Platform TLV starts with "Meraki <model>".
    #[test]
    fn meraki_platform_recognized() {
        assert!(is_meraki_platform(b"Meraki MX84"));
        assert!(is_meraki_platform(b"Meraki MR42 Cloud Managed AP"));
        assert!(is_meraki_platform(b"Meraki MS220-8"));
        assert!(!is_meraki_platform(b"cisco WS-C2960"));
        assert!(!is_meraki_platform(b""));
        assert!(!is_meraki_platform(b"Meraki"));
        assert!(!is_meraki_platform(b"Meraki "));
    }

    /// Meraki Device ID TLV matches `<model><serial>`.
    #[test]
    fn meraki_device_id_recognized() {
        assert!(is_meraki_device_id(b"MX841234ABCD"));
        assert!(is_meraki_device_id(b"MR42ABC123"));
        assert!(!is_meraki_device_id(b"WS-C2960-X"));
        assert!(!is_meraki_device_id(b""));
        assert!(!is_meraki_device_id(b"MX"));
    }

    /// Reject a CDP frame shorter than the 4-byte header.
    #[test]
    fn parse_rejects_short_header() {
        assert!(parse(&[]).is_err());
        assert!(parse(&[0x02, 0xb4, 0x00]).is_err());
    }

    /// Reject a TLV whose declared length exceeds the remaining
    /// input. Build a frame whose last TLV lies about its length.
    #[test]
    fn parse_rejects_truncated_tlv() {
        // Version(1)+TTL(1)+Checksum(2) + TLV type=0x0001 length=999
        let bytes = vec![0x02, 0xb4, 0x00, 0x00, 0x00, 0x01, 0x03, 0xE7];
        assert!(parse(&bytes).is_err());
    }

    /// Empty TLV region (no TLVs after the 4-byte header) parses
    /// successfully and returns an empty list.
    #[test]
    fn parse_empty_tlv_region() {
        let bytes = vec![0x02, 0xb4, 0x00, 0x00];
        let tlvs = parse(&bytes).expect("empty TLV region is valid");
        assert!(tlvs.is_empty());
    }

    /// `parse_tlvs` operates on the TLV region only (no header).
    #[test]
    fn parse_tlvs_without_header() {
        let bytes = cdp_fixture();
        let tlvs = parse_tlvs(&bytes[4..]).expect("parse_tlvs should succeed");
        assert_eq!(tlvs.len(), 3);
        assert_eq!(tlvs[0].type_id, TLV_DEVICE_ID);
        assert_eq!(tlvs[1].type_id, TLV_PLATFORM);
        assert_eq!(tlvs[2].type_id, TLV_PORT_ID);
    }

    /// Verify a synthetic non-Meraki CDP frame parses correctly and
    /// is correctly classified as non-Meraki by the OUI/platform
    /// helpers.
    #[test]
    fn parse_non_meraki_cdp() {
        let mut bytes = vec![0x02, 0xb4, 0x00, 0x00];
        // TLV 0x0006 Platform = b"cisco WS-C2960-24"
        let plat = b"cisco WS-C2960-24";
        bytes.extend_from_slice(&TLV_PLATFORM.to_be_bytes());
        bytes.extend_from_slice(&(plat.len() as u16).to_be_bytes());
        bytes.extend_from_slice(plat);
        let tlvs = parse(&bytes).unwrap();
        assert_eq!(tlvs.len(), 1);
        assert_eq!(tlvs[0].type_id, TLV_PLATFORM);
        assert_eq!(tlvs[0].value, plat);
        assert!(!is_meraki_platform(&tlvs[0].value));
    }
}
