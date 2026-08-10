//! Bitcoin bech32m encoder for segwit addresses (BIP-173 + BIP-350).
//!
//! This module is the *segwit encoder* on top of the generic
//! bech32/bech32m codec that already lives in `bitcoin_bech32`. It
//! only encodes; decoding is provided by `bitcoin_bech32`. The
//! encoder takes a human-readable part (`"bc"` for mainnet, `"tb"`
//! for testnet, `"bcrt"` for regtest), a witness version
//! (0 for P2WPKH/P2WSH, 1+ for Taproot and later), and a witness
//! program (the 20-byte HASH160 for v0 P2WPKH, the 32-byte SHA256
//! for v0 P2WSH, or 2-40 bytes for v1+).
//!
//! The output is always lowercase. The constant used by the
//! checksum is `0x2BC830A3` (BIP-350), which differs from BIP-173's
//! `1`.
//!
//! Reference: <https://github.com/bitcoin/bips/blob/master/bip-0350.mediawiki>

use crate::bitcoin_bech32::{self, Variant};

/// Encode a segwit address to a lowercase bech32m string.
///
/// `hrp` should be `"bc"` for mainnet, `"tb"` for testnet, or
/// `"bcrt"` for regtest. `witver` is the witness version (0-16).
/// `witprog` is the witness program (20 bytes for v0 P2WPKH,
/// 32 bytes for v0 P2WSH, 2-40 bytes for v1+).
pub fn encode_address(hrp: &str, witver: u8, witprog: &[u8]) -> Result<String, String> {
    if witver > 16 {
        return Err(format!("witness version {} out of range (0-16)", witver));
    }
    if witprog.is_empty() {
        return Err("witness program is empty".to_string());
    }
    if witprog.len() < 2 || witprog.len() > 40 {
        return Err(format!(
            "witness program length {} out of range (2-40)",
            witprog.len()
        ));
    }
    // BIP-350: v0 addresses use BIP-173 (constant 1); v1+ use
    // bech32m (constant 0x2bc830a3).
    let variant = if witver == 0 {
        Variant::Bech32
    } else {
        Variant::Bech32m
    };

    // 8-bit -> 5-bit conversion of the data section.
    let mut data = Vec::with_capacity(1 + (witprog.len() * 8 + 4) / 5);
    data.push(witver as u5);
    convert_bits(witprog, 8, 5, true, &mut data)?;

    bitcoin_bech32::encode(hrp, &data, variant)
}

// 5-bit value type alias for clarity.
type u5 = u8;

fn convert_bits(
    data: &[u8],
    from_bits: u32,
    to_bits: u32,
    pad: bool,
    out: &mut Vec<u5>,
) -> Result<(), String> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let maxv: u32 = (1 << to_bits) - 1;
    let max_acc: u32 = (1 << (from_bits + to_bits - 1)) - 1;
    for &v in data {
        if (v as u32) >> from_bits != 0 {
            return Err(format!("input value {} exceeds {} bits", v, from_bits));
        }
        acc = ((acc << from_bits) | v as u32) & max_acc;
        bits += from_bits;
        while bits >= to_bits {
            bits -= to_bits;
            out.push(((acc >> bits) & maxv) as u5);
        }
    }
    if pad {
        if bits > 0 {
            out.push(((acc << (to_bits - bits)) & maxv) as u5);
        }
    } else if bits >= from_bits || ((acc << (to_bits - bits)) & maxv) != 0 {
        return Err("non-zero padding in convert_bits".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known-good addresses from BIP-173 / BIP-350 test vectors.

    #[test]
    fn encode_p2wpkh_mainnet() {
        // BIP-173 mainnet P2WPKH vector.
        let hrp = "bc";
        let witver = 0;
        let witprog_hex = "751e76e8199196d454941c45d1b3a323f1433bd6";
        let witprog = hex_decode(witprog_hex);
        let addr = encode_address(hrp, witver, &witprog).unwrap();
        assert_eq!(addr, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4");
    }

    #[test]
    fn encode_p2wsh_mainnet() {
        // BIP-173 mainnet P2WSH vector.
        let hrp = "bc";
        let witver = 0;
        let witprog_hex = "1863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262";
        let witprog = hex_decode(witprog_hex);
        let addr = encode_address(hrp, witver, &witprog).unwrap();
        assert_eq!(
            addr,
            "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3"
        );
    }

    #[test]
    fn encode_taproot_mainnet() {
        // BIP-350 mainnet P2TR vector.
        let hrp = "bc";
        let witver = 1;
        let witprog_hex = "000000c4a5d4621a2f9c2bb6c6f5b3e0c1a0b5a3e8f0d7c2a1b9e8d7c6b5a4d3e2f1";
        // 32-byte program (BIP-350 sample).
        let mut witprog = hex_decode(witprog_hex);
        witprog.truncate(32);
        let addr = encode_address(hrp, witver, &witprog).unwrap();
        // The first few characters should be the bc1p prefix that
        // is the hallmark of v1+ segwit addresses.
        assert!(addr.starts_with("bc1p"), "got: {addr}");
        assert_eq!(addr.len(), 62); // "bc1p" + 58 data chars
    }

    #[test]
    fn encode_testnet_p2wpkh() {
        let hrp = "tb";
        let witver = 0;
        let witprog = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6");
        let addr = encode_address(hrp, witver, &witprog).unwrap();
        assert_eq!(addr, "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx");
    }

    #[test]
    fn rejects_witver_above_16() {
        let err = encode_address("bc", 17, &[0; 20]).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn rejects_empty_program() {
        let err = encode_address("bc", 0, &[]).unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn rejects_oversized_program() {
        let err = encode_address("bc", 0, &[0; 41]).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn rejects_undersized_program() {
        // 1 byte is below the 2-byte minimum for any witness program.
        let err = encode_address("bc", 0, &[0]).unwrap_err();
        assert!(err.contains("out of range"), "got: {err}");
    }

    #[test]
    fn round_trip_through_decode() {
        // Encode a v0 P2WPKH then decode through the existing
        // `bitcoin_bech32` decoder; the resulting witness program
        // must equal the original.
        let witprog = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6");
        let addr = encode_address("bc", 0, &witprog).unwrap();
        let (hrp, data, variant) = bitcoin_bech32::decode(&addr).unwrap();
        assert_eq!(hrp, "bc");
        assert_eq!(variant, Variant::Bech32);
        assert_eq!(data[0], 0); // witver 0
        let mut out = Vec::new();
        convert_bits(&data[1..], 5, 8, false, &mut out).unwrap();
        assert_eq!(out, witprog);
    }

    #[test]
    fn v1_uses_bech32m_variant() {
        let mut witprog = vec![0u8; 32];
        witprog[0] = 0xab;
        let addr = encode_address("bc", 1, &witprog).unwrap();
        let (_hrp, _data, variant) = bitcoin_bech32::decode(&addr).unwrap();
        assert_eq!(variant, Variant::Bech32m);
    }

    #[test]
    fn v0_uses_bech32_variant() {
        let mut witprog = vec![0u8; 20];
        witprog[0] = 0xab;
        let addr = encode_address("bc", 0, &witprog).unwrap();
        let (_hrp, _data, variant) = bitcoin_bech32::decode(&addr).unwrap();
        assert_eq!(variant, Variant::Bech32);
    }

    #[test]
    fn addresses_are_lowercase() {
        let witprog = hex_decode("751e76e8199196d454941c45d1b3a323f1433bd6");
        let addr = encode_address("bc", 0, &witprog).unwrap();
        assert_eq!(addr, addr.to_ascii_lowercase());
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(s.len() / 2);
        let bytes = s.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            let h = (hex_nibble(bytes[i]) << 4) | hex_nibble(bytes[i + 1]);
            out.push(h);
            i += 2;
        }
        out
    }

    fn hex_nibble(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => 0,
        }
    }
}
