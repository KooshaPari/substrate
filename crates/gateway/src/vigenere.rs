//! Vigenère cipher encoder/decoder.
//!
//! Polyalphabetic substitution cipher where each letter is shifted by a
//! different amount based on a repeating key. The Caesar cipher is
//! the special case of a single-character key.
//!
//! [`encode`] and [`decode`] are inverse operations with the same key.
//! Non-letters are passed through unchanged and do not advance the
//! key index.

/// Vigenère cipher shift by `key`. Letters are shifted by the
/// corresponding key letter's alphabet position (A=0, B=1, ..., Z=25);
/// non-letters are preserved as-is without consuming a key position.
///
/// Examples:
/// - encode("HELLO", "KEY") = "RIJVS"
/// - encode("ATTACKATDAWN", "LEMON") = "LXFOPVEFRNHR"
pub fn encode(s: &str, key: &str) -> String {
    transform(s, key, false)
}

/// Vigenère cipher decode (inverse of [`encode`]).
pub fn decode(s: &str, key: &str) -> String {
    transform(s, key, true)
}

fn transform(s: &str, key: &str, decode: bool) -> String {
    if key.is_empty() {
        return s.to_string();
    }
    let key_upper: Vec<u8> = key
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase() as u8 - b'A')
        .collect();
    if key_upper.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut key_idx = 0;
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            let shift = key_upper[key_idx % key_upper.len()];
            let shift = if decode { 26 - shift } else { shift };
            let idx = (c as u8 - b'A' + shift) % 26;
            out.push((b'A' + idx) as char);
            key_idx += 1;
        } else if c.is_ascii_lowercase() {
            let shift = key_upper[key_idx % key_upper.len()];
            let shift = if decode { 26 - shift } else { shift };
            let idx = (c as u8 - b'a' + shift) % 26;
            out.push((b'a' + idx) as char);
            key_idx += 1;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_classic() {
        assert_eq!(encode("HELLO", "KEY"), "RIJVS");
    }

    #[test]
    fn encode_attack_at_dawn() {
        // Famous example from cryptography history
        assert_eq!(encode("ATTACKATDAWN", "LEMON"), "LXFOPVEFRNHR");
    }

    #[test]
    fn decode_is_inverse() {
        let original = "Hello, World!";
        for key in ["KEY", "LEMON", "ABC", "XYZ"] {
            let encoded = encode(original, key);
            let decoded = decode(&encoded, key);
            assert_eq!(decoded, original, "key={key}");
        }
    }

    #[test]
    fn empty_key_returns_input() {
        assert_eq!(encode("HELLO", ""), "HELLO");
        assert_eq!(decode("HELLO", ""), "HELLO");
    }

    #[test]
    fn preserves_non_letters() {
        // Spaces and punctuation don't advance key.
        // H E L L O , [no-advance] W O R L D !
        // K E Y K E     Y   K E Y K   →   R I J V S , U Y V J N !
        assert_eq!(encode("HELLO, WORLD!", "KEY"), "RIJVS, UYVJN!");
    }

    #[test]
    fn mixed_case_handled() {
        assert_eq!(encode("Hello", "KEY"), "Rijvs");
    }

    #[test]
    fn single_char_key_reduces_to_caesar() {
        // Vigenère with "K" (key letter = 10) = Caesar shift 10
        assert_eq!(encode("HELLO", "K"), "ROVVY");
    }
}
