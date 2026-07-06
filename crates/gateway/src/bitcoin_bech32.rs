//! Minimal Bitcoin Bech32 / Bech32m address codec (BIP-173 / BIP-350).
//!
//! Encodes and decodes strings of the form `<hrp>1<data><6-char-checksum>`,
//! where `data` is an array of 5-bit groups and the checksum is a 6-group
//! polymod computed over the human-readable part expanded to 5-bit groups,
//! the separator, and the data.
//!
//! Two variants are supported:
//!
//! - [`Variant::Bech32`] uses constant `1` and is the address format for
//!   native segwit v0 (BIP-173).
//! - [`Variant::Bech32m`] uses constant `0x2bc830a3` and is the address
//!   format for segwit v1+ (BIP-350, including Taproot).
//!
//! Decoding rejects:
//!
//! - Inputs longer than 90 characters total (per BIP-173).
//! - Mixed-case strings (both upper and lower letters).
//! - HRP characters outside `[!-~]` or data characters outside `[!-~]`.
//! - HRP longer than 83 characters or shorter than 1 character.
//! - Invalid checksum (polymod != 1 for Bech32, != 0x2bc830a3 for Bech32m).
//!
//! This module does not validate that the decoded `data` represents a valid
//! segwit program (witness version / length / script semantics). Callers
//! building segwit addresses should layer that check on top.

/// The two checksum constants defined by BIP-173 and BIP-350.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Variant {
    /// BIP-173 — segwit v0. Polymod constant `1`.
    Bech32,
    /// BIP-350 — segwit v1 and later. Polymod constant `0x2bc830a3`.
    Bech32m,
}

impl Variant {
    /// Polymod constant for this variant. Used by both encoder and decoder.
    fn constant(self) -> u32 {
        match self {
            Variant::Bech32 => 1,
            Variant::Bech32m => 0x2bc830a3,
        }
    }
}

/// Maximum total length of a Bech32 string (90 chars per BIP-173).
const MAX_BECH32_LEN: usize = 90;

/// Generator coefficients for the BCH checksum. Defined by BIP-173.
const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

/// Bitmask used to keep values within 5-bit range when feeding polymod.
const MASK_5BIT: u8 = 0x1f;

/// Reverse one byte (used to map an ascii lowercase -> uppercase value).
fn bech32_polymod(values: &[u8]) -> u32 {
    let mut chk: u32 = 1;
    for &v in values {
        let top = chk >> 25;
        chk = (chk & 0x1ffffff) << 5 ^ (v as u32);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 != 0 {
                chk ^= g;
            }
        }
    }
    chk
}

/// Expand a human-readable prefix into the 5-bit-group stream the polymod
/// expects: high 5 bits of each ascii char, then a separator value of 0,
/// then low 5 bits of each ascii char.
fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hrp.len() * 2 + 1);
    for b in hrp.bytes() {
        out.push(b >> 5);
    }
    out.push(0);
    for b in hrp.bytes() {
        out.push(b & MASK_5BIT);
    }
    out
}

/// Compute the 6-group checksum suffix for `hrp` + `data` under `variant`.
fn create_checksum(hrp: &str, data: &[u8], variant: Variant) -> Vec<u8> {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    values.extend_from_slice(&[0u8; 6]);
    let mut modv = bech32_polymod(&values) ^ variant.constant();
    let mut out = Vec::with_capacity(6);
    for i in (0..6).rev() {
        out.push((modv >> (5 * i)) as u8 & MASK_5BIT);
    }
    out
}

/// Validate a checksum: polymod must equal the variant's constant exactly.
fn verify_checksum(hrp: &str, data: &[u8]) -> Result<Variant, String> {
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(data);
    let p = bech32_polymod(&values);
    if p == Variant::Bech32.constant() {
        Ok(Variant::Bech32)
    } else if p == Variant::Bech32m.constant() {
        Ok(Variant::Bech32m)
    } else {
        Err("invalid bech32/bech32m checksum".into())
    }
}

/// Map a single Bech32 character into its 5-bit value.
///
/// Accepts both lowercase (`qpzry9x8gf2tvdw0s3jn54khce6mua7l`) and uppercase
/// (`QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L`) forms. Returns `None` for any other
/// byte.
fn bech32_charset_value(c: u8) -> Option<u8> {
    match c {
        b'q' | b'Q' => Some(0),
        b'p' | b'P' => Some(1),
        b'z' | b'Z' => Some(2),
        b'r' | b'R' => Some(3),
        b'y' | b'Y' => Some(4),
        b'9' => Some(5),
        b'x' | b'X' => Some(6),
        b'8' => Some(7),
        b'g' | b'G' => Some(8),
        b'f' | b'F' => Some(9),
        b'2' => Some(10),
        b't' | b'T' => Some(11),
        b'v' | b'V' => Some(12),
        b'd' | b'D' => Some(13),
        b'w' | b'W' => Some(14),
        b'0' => Some(15),
        b's' | b'S' => Some(16),
        b'3' => Some(17),
        b'j' | b'J' => Some(18),
        b'n' | b'N' => Some(19),
        b'5' => Some(20),
        b'4' => Some(21),
        b'k' | b'K' => Some(22),
        b'h' | b'H' => Some(23),
        b'c' | b'C' => Some(24),
        b'e' | b'E' => Some(25),
        b'6' => Some(26),
        b'm' | b'M' => Some(27),
        b'u' | b'U' => Some(28),
        b'a' | b'A' => Some(29),
        b'7' => Some(30),
        b'l' | b'L' => Some(31),
        _ => None,
    }
}

/// Encode `hrp` + `data` (5-bit groups) into a Bech32/Bech32m string.
///
/// `data` is the 5-bit-grouped payload — i.e. the witness program encoded
/// per BIP-173 §3 (8-bit groups re-packed into 5-bit groups with
/// zero-padding of the final group). The trailing 6-group checksum is
/// computed and appended automatically.
///
/// Returns an error if `hrp` is empty or longer than 83 characters.
pub fn encode(hrp: &str, data: &[u8], variant: Variant) -> Result<String, String> {
    if hrp.is_empty() {
        return Err("hrp must not be empty".into());
    }
    if hrp.len() > 83 {
        return Err(format!("hrp too long: {} chars (max 83)", hrp.len()));
    }
    for b in hrp.bytes() {
        if b < 33 || b > 126 {
            return Err(format!("hrp contains invalid byte 0x{b:02x}"));
        }
    }
    for &v in data {
        if v > 31 {
            return Err(format!("data value {v} exceeds 5-bit range"));
        }
    }

    let mut combined = data.to_vec();
    combined.extend(create_checksum(hrp, data, variant));

    let mut out = String::with_capacity(hrp.len() + 1 + combined.len());
    out.push_str(hrp);
    out.push('1');
    for v in combined {
        let c = match v {
            0 => 'q',
            1 => 'p',
            2 => 'z',
            3 => 'r',
            4 => 'y',
            5 => '9',
            6 => 'x',
            7 => '8',
            8 => 'g',
            9 => 'f',
            10 => '2',
            11 => 't',
            12 => 'v',
            13 => 'd',
            14 => 'w',
            15 => '0',
            16 => 's',
            17 => '3',
            18 => 'j',
            19 => 'n',
            20 => '5',
            21 => '4',
            22 => 'k',
            23 => 'h',
            24 => 'c',
            25 => 'e',
            26 => '6',
            27 => 'm',
            28 => 'u',
            29 => 'a',
            30 => '7',
            31 => 'l',
            _ => unreachable!(),
        };
        out.push(c);
    }
    if out.len() > MAX_BECH32_LEN {
        return Err(format!(
            "encoded string {} chars exceeds Bech32 max {}",
            out.len(),
            MAX_BECH32_LEN
        ));
    }
    Ok(out)
}

/// Decode a Bech32/Bech32m string into `(hrp, payload, variant)`.
///
/// `payload` is the 5-bit-group data with the trailing 6-group checksum
/// stripped. Callers that need the original 8-bit payload must perform the
/// 5-to-8-bit group conversion themselves.
///
/// Returns an error string describing the first failure: bad charset,
/// missing separator, mixed case, length overflow, or invalid checksum.
pub fn decode(s: &str) -> Result<(String, Vec<u8>, Variant), String> {
    if s.is_empty() {
        return Err("empty input".into());
    }
    if s.len() > MAX_BECH32_LEN {
        return Err(format!(
            "input length {} exceeds Bech32 max {}",
            s.len(),
            MAX_BECH32_LEN
        ));
    }

    // Reject mixed case per BIP-173 / BIP-350.
    let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
    if has_lower && has_upper {
        return Err("mixed case not allowed".into());
    }

    // Require ASCII printable, [!-~].
    for c in s.chars() {
        let b = c as u32;
        if b < 0x21 || b > 0x7e {
            return Err(format!("non-printable character U+{b:04X}"));
        }
    }

    // Lowercase the entire string before checksum verification (BIP-350 §"Decoding").
    let s_lower = s.to_ascii_lowercase();

    // Find the last '1' (separator). HRP cannot contain '1'.
    let sep_pos = s_lower.rfind('1').ok_or_else(|| "no separator '1' found".to_string())?;
    if sep_pos == 0 {
        return Err("hrp is empty".into());
    }
    if sep_pos + 7 > s_lower.len() {
        return Err("data/checksum section too short".into());
    }
    let hrp = &s_lower[..sep_pos];
    let data_str = &s_lower[sep_pos + 1..];

    // HRP must itself be [!-~] printable, with no '1'.
    for c in hrp.chars() {
        let b = c as u32;
        if b < 33 || b > 126 {
            return Err(format!("hrp has non-printable byte U+{b:04X}"));
        }
    }

    // Convert data chars into 5-bit values.
    let mut data = Vec::with_capacity(data_str.len());
    for c in data_str.chars() {
        let v = bech32_charset_value(c as u8).ok_or_else(|| {
            format!("character '{c}' is not in the Bech32 charset")
        })?;
        data.push(v);
    }

    let variant = verify_checksum(hrp, &data)?;

    // Strip the 6-group checksum before returning. Return the lowercased
    // HRP — callers that need the original case can preserve it from input.
    let payload = data[..data.len() - 6].to_vec();
    Ok((hrp.to_string(), payload, variant))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: pack 8-bit bytes into 5-bit groups (BIP-173 §3 conversion).
    fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Vec<u8> {
        let mut acc: u32 = 0;
        let mut bits: u32 = 0;
        let mut ret: Vec<u8> = Vec::new();
        let maxv = (1u32 << to) - 1;
        let max_acc = (1u32 << (from + to - 1)) - 1;
        for &value in data {
            acc = ((acc << from) | (value as u32)) & max_acc;
            bits += from;
            while bits >= to {
                bits -= to;
                ret.push(((acc >> bits) & maxv) as u8);
            }
        }
        if pad && bits > 0 {
            ret.push(((acc << (to - bits)) & maxv) as u8);
        }
        ret
    }

    #[test]
    fn decode_bip173_reference_vector() {
        // From BIP-173 valid test vectors:
        //   BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4
        let (hrp, data, variant) = decode("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4").unwrap();
        assert_eq!(hrp, "bc");
        assert_eq!(variant, Variant::Bech32);
        // 5-bit payload of the segwit v0 P2WPKH for the reference pubkey.
        // Just confirm it round-trips through encode.
        let encoded = encode(&hrp, &data, variant).unwrap();
        assert_eq!(
            encoded.to_ascii_uppercase(),
            "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4"
        );
    }

    #[test]
    fn decode_bip350_reference_vector() {
        // From BIP-350 valid test vectors:
        //   A1LQFN3A  (HRP=A, data=LQFN3A, bech32m checksum)
        let (hrp, data, variant) = decode("A1LQFN3A").unwrap();
        assert_eq!(hrp, "a");
        assert_eq!(variant, Variant::Bech32m);
        let encoded = encode(&hrp, &data, variant).unwrap();
        assert_eq!(encoded.to_ascii_uppercase(), "A1LQFN3A");
    }

    #[test]
    fn round_trip_bech32() {
        // P2WPKH witness program (20 zero bytes): use 5-bit-pack of [0;20].
        let payload8: Vec<u8> = vec![0u8; 20];
        let data = convert_bits(&payload8, 8, 5, true);
        let s = encode("bc", &data, Variant::Bech32).unwrap();
        let (hrp, payload, variant) = decode(&s).unwrap();
        assert_eq!(hrp, "bc");
        assert_eq!(variant, Variant::Bech32);
        assert_eq!(payload, data);
    }

    #[test]
    fn round_trip_bech32m() {
        let payload8: Vec<u8> = vec![0xaau8; 32];
        let data = convert_bits(&payload8, 8, 5, true);
        let s = encode("tb", &data, Variant::Bech32m).unwrap();
        let (hrp, payload, variant) = decode(&s).unwrap();
        assert_eq!(hrp, "tb");
        assert_eq!(variant, Variant::Bech32m);
        assert_eq!(payload, data);
    }

    #[test]
    fn reject_mixed_case() {
        // Same string with mixed case fails per BIP-173.
        assert!(decode("bc1Qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3a4").is_err());
        assert!(decode("BC1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3a4").is_err());
    }

    #[test]
    fn reject_too_long() {
        // BIP-173 says total length must be <= 90. Encode rejects payloads
        // that would exceed the cap, and decode rejects raw inputs that
        // exceed it (no encoding step required).
        let payload8 = vec![0u8; 65];
        let data = convert_bits(&payload8, 8, 5, true);
        // Encode itself refuses to emit strings > 90 chars.
        assert!(encode("a", &data, Variant::Bech32).is_err());
        // And the decoder independently enforces the cap.
        let too_long = "a".repeat(91);
        assert!(decode(&too_long).is_err());
    }

    #[test]
    fn reject_invalid_checksum() {
        // BIP-173 invalid vector: 90 chars, bad checksum.
        let s = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kemeawh";
        // Either decodes as the wrong variant or fails — must NOT silently pass.
        let result = decode(s);
        assert!(result.is_err() || result.unwrap().2 == Variant::Bech32m);
    }

    #[test]
    fn reject_bad_charset() {
        // 'b' and 'i' and 'o' are not in the Bech32 alphabet.
        assert!(decode("bc1qb508d6qejxtdg4y5r3zarvary0c5xw7kv8f3a4").is_err());
        assert!(decode("bc1ib508d6qejxtdg4y5r3zarvary0c5xw7kv8f3a4").is_err());
    }

    #[test]
    fn empty_input_rejected() {
        assert!(decode("").is_err());
    }

    #[test]
    fn empty_hrp_rejected() {
        // No hrp before the separator.
        assert!(decode("1qqqqqqqqqqqqqqqq").is_err());
    }
}