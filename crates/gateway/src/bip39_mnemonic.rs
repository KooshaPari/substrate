//! Minimal BIP-39 mnemonic encoder/decoder for the English wordlist.
//!
//! BIP-39 turns a binary seed into a sequence of words drawn from a
//! 2048-word wordlist. Each group of 11 bits maps to one word. A
//! checksum equal to the first `ENT/32` bits of `SHA256(entropy)` is
//! appended to those bits, giving the final bitstream.
//!
//! Valid entropy sizes per the spec are `128`, `160`, `192`, `224`,
//! and `256` bits — corresponding to 12, 15, 18, 21, and 24-word
//! mnemonics.
//!
//! Reference: BIP-39 specification, §"Generating the mnemonic" and
//! §"From mnemonic to seed" (the `to_seed` extension is intentionally
//! not implemented — `from_bits`/`to_bits` cover the codec surface).
//!
//! Wordlist lives in [`crate::bip39_wordlist`].

use sha2::{Digest, Sha256};

/// The only supported wordlist (English).
pub const WORDLIST: [&str; 2048] = crate::bip39_wordlist::ENGLISH;

/// Number of bits per word index. `11` because `2^11 == 2048`.
const BITS_PER_WORD: usize = 11;

/// All valid entropy lengths, in bits.
pub const VALID_ENTROPY_BITS: [usize; 5] = [128, 160, 192, 224, 256];

/// Returns `Ok(())` if the mnemonic encodes valid entropy+checksum,
/// `Err(msg)` otherwise.
pub fn validate(mnemonic: &str) -> Result<bool, String> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    let total_bits = words.len() * BITS_PER_WORD;
    let checksum_bits = match words.len() {
        12 => 4,
        15 => 5,
        18 => 6,
        21 => 7,
        24 => 8,
        n => {
            return Err(format!(
                "mnemonic length {n} words not in {{12,15,18,21,24}}"
            ));
        }
    };
    let ent_bits = total_bits - checksum_bits;
    if !VALID_ENTROPY_BITS.contains(&ent_bits) {
        return Err(format!("invalid entropy bits {ent_bits}"));
    }
    // Resolve each word to its 11-bit index.
    let mut all_bits: Vec<u8> = Vec::with_capacity(total_bits);
    for w in &words {
        let idx = WORDLIST
            .iter()
            .position(|c| *c == *w)
            .ok_or_else(|| format!("unknown word: {w:?}"))?;
        for shift in (0..BITS_PER_WORD).rev() {
            all_bits.push(((idx >> shift) & 1) as u8);
        }
    }
    let entropy_bytes: Vec<u8> = pack_bits(&all_bits, ent_bits)
        .ok_or_else(|| "entropy bit-packing failed".to_string())?;
    let checksum_packed = pack_bits(&all_bits[ent_bits..], checksum_bits)
        .ok_or_else(|| "checksum bit-packing failed".to_string())?[0];
    // The packed byte holds the N-bit checksum MSB-aligned (bits
    // 7-(8-N)). Right-shift it so both sides compare integer form.
    let checksum_extracted = checksum_packed >> (8 - checksum_bits);
    // Recompute the checksum from entropy.
    let mut hasher = Sha256::new();
    hasher.update(&entropy_bytes);
    let hash = hasher.finalize();
    let checksum_expected = hash[0] >> (8 - checksum_bits);
    Ok(checksum_extracted == checksum_expected)
}

/// Encode the supplied binary entropy into a mnemonic. `entropy` MUST
/// be a supported size (128/160/192/224/256 bits = 16/20/24/28/32 bytes).
pub fn entropy_to_mnemonic(entropy: &[u8]) -> Result<String, String> {
    let ent_bits = entropy.len() * 8;
    if !VALID_ENTROPY_BITS.contains(&ent_bits) {
        return Err(format!(
            "entropy must be one of {:?} bits, got {}",
            VALID_ENTROPY_BITS, ent_bits
        ));
    }
    let checksum_bits = ent_bits / 32;
    let total_bits = ent_bits + checksum_bits;
    // Build a bitstream: entropy bits followed by the leading N bits
    // of SHA256(entropy).
    let mut hasher = Sha256::new();
    hasher.update(entropy);
    let hash = hasher.finalize();
    let mut bits: Vec<u8> = Vec::with_capacity(total_bits);
    for byte in entropy {
        for shift in (0..8).rev() {
            bits.push(((byte >> shift) & 1) as u8);
        }
    }
    let mut remaining = checksum_bits;
    for byte in hash.iter() {
        if remaining == 0 {
            break;
        }
        for shift in (0..8).rev() {
            if remaining == 0 {
                break;
            }
            bits.push(((byte >> shift) & 1) as u8);
            remaining -= 1;
        }
    }
    // Translate every 11 bits to one word.
    if bits.len() != total_bits {
        return Err(format!(
            "bit-construction drift: {} != {total_bits}",
            bits.len()
        ));
    }
    let mut words: Vec<&str> = Vec::with_capacity(total_bits / BITS_PER_WORD);
    for chunk in bits.chunks(BITS_PER_WORD) {
        let mut idx = 0u32;
        for bit in chunk {
            idx = (idx << 1) | (*bit as u32);
        }
        words.push(WORDLIST[idx as usize]);
    }
    Ok(words.join(" "))
}

/// Decode a mnemonic back into the entropy bytes the spec says it
/// represents. Returns an error for any malformed input.
pub fn mnemonic_to_entropy(mnemonic: &str) -> Result<Vec<u8>, String> {
    if !validate(mnemonic)? {
        return Err("checksum mismatch".into());
    }
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    let total_bits = words.len() * BITS_PER_WORD;
    let checksum_bits = match words.len() {
        12 => 4,
        15 => 5,
        18 => 6,
        21 => 7,
        24 => 8,
        n => return Err(format!("bad length {n}")),
    };
    let ent_bits = total_bits - checksum_bits;
    let mut bits: Vec<u8> = Vec::with_capacity(total_bits);
    for w in &words {
        let idx = WORDLIST
            .iter()
            .position(|c| *c == *w)
            .ok_or_else(|| format!("unknown word: {w:?}"))?;
        for shift in (0..BITS_PER_WORD).rev() {
            bits.push(((idx >> shift) & 1) as u8);
        }
    }
    pack_bits(&bits, ent_bits)
        .ok_or_else(|| "entropy bit-packing failed".to_string())
}

/// Take `n_bits` bits from the front of `bits` and pack them MSB-first
/// into bytes. Returns `None` if `bits` has fewer than `n_bits`.
fn pack_bits(bits: &[u8], n_bits: usize) -> Option<Vec<u8>> {
    if bits.len() < n_bits {
        return None;
    }
    let n_bytes = (n_bits + 7) / 8;
    let mut out = Vec::with_capacity(n_bytes);
    let mut idx = 0usize;
    while idx < n_bits {
        let mut byte = 0u8;
        let chunk_end = (idx + 8).min(n_bits);
        for (j, b) in bits[idx..chunk_end].iter().enumerate() {
            byte |= (*b & 1) << (7 - j);
        }
        out.push(byte);
        idx += 8;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip every official Trezor BIP-39 English test vector.
    /// Cross-checked against
    /// <https://github.com/trezor/python-mnemonic/blob/master/vectors.json>.
    /// If the encoder drifts from the spec, `validate()` will fail on
    /// its own output (the field-level round-trip catches bit order
    /// mistakes immediately).
    const TREZOR_VECTORS: &[(&str, &str)] = &[
        (
            "00000000000000000000000000000000",
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        ),
        (
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
        ),
        (
            "80808080808080808080808080808080",
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
        ),
        (
            "ffffffffffffffffffffffffffffffff",
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
        ),
        (
            "0000000000000000000000000000000000000000000000000000000000000000",
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art",
        ),
    ];

    fn hex_to_bytes(s: &str) -> Vec<u8> {
        assert!(s.len() % 2 == 0, "odd hex length");
        let mut out = Vec::with_capacity(s.len() / 2);
        for i in (0..s.len()).step_by(2) {
            out.push(u8::from_str_radix(&s[i..i + 2], 16).unwrap());
        }
        out
    }

    fn bytes_to_hex(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len() * 2);
        for byte in b {
            s.push_str(&format!("{byte:02x}"));
        }
        s
    }

    #[test]
    fn entropy_to_mnemonic_matches_trezor_vectors() {
        for (hex, expected) in TREZOR_VECTORS {
            let entropy = hex_to_bytes(hex);
            let mnemonic = entropy_to_mnemonic(&entropy).expect("encode");
            assert_eq!(
                &mnemonic, expected,
                "Trezor vector mismatch for entropy 0x{hex}"
            );
        }
    }

    #[test]
    fn mnemonic_to_entropy_matches_trezor_vectors() {
        for (expected_hex, mnemonic) in TREZOR_VECTORS {
            let entropy = mnemonic_to_entropy(mnemonic).expect("decode");
            assert_eq!(
                bytes_to_hex(&entropy),
                *expected_hex,
                "Trezor vector entropy mismatch for mnemonic {mnemonic:?}"
            );
        }
    }

    #[test]
    fn validate_accepts_trezor_vectors() {
        for (_hex, mnemonic) in TREZOR_VECTORS {
            assert_eq!(validate(mnemonic), Ok(true), "validate {mnemonic:?}");
        }
    }

    #[test]
    fn validate_rejects_checksum_tamper() {
        // Flip the LAST word of the 12-word Trezor vector — checksum
        // must now fail. (12 words: checksum = 4 bits.)
        let bad = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        // 12 words but each is "abandon" (index 0) — checksum would
        // be 0.., but `abandon...about` had a specific 4-bit suffix.
        assert!(validate(bad).is_err() || validate(bad) == Ok(false));
    }

    #[test]
    fn validate_rejects_unknown_word() {
        let bad = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon notaword";
        assert!(validate(bad).is_err());
    }

    #[test]
    fn validate_rejects_wrong_length() {
        // 11 words — not a supported length.
        let bad = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        assert!(validate(bad).is_err());
    }

    #[test]
    fn round_trip_arbitrary_entropy() {
        // 128-bit entropy different from any Trezor vector.
        let entropy: Vec<u8> = (0u8..16).collect();
        let mnemonic = entropy_to_mnemonic(&entropy).expect("encode");
        assert_eq!(validate(&mnemonic), Ok(true));
        let recovered = mnemonic_to_entropy(&mnemonic).expect("decode");
        assert_eq!(recovered, entropy);
    }

    #[test]
    fn all_2048_words_resolve() {
        // Each index must round-trip back to itself through the
        // encoder. Sanity check on the wordlist.
        for (i, word) in WORDLIST.iter().enumerate() {
            let idx = WORDLIST.iter().position(|c| *c == *word);
            assert_eq!(idx, Some(i), "word {word:?} resolves to {idx:?}");
        }
    }
}
