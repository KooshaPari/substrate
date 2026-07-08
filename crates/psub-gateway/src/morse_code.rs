//! Morse code encoder/decoder.
//!
//! Standard ITU Morse code table for ASCII letters and digits. Useful
//! for shortwave radio, ham radio practice, and CTF-style encoding
//! puzzles. Punctuation is intentionally NOT supported — callers should
//! preprocess or encode punctuation separately.
//!
//! Reference: ITU-T Recommendation M.1677 (2009).

use std::collections::HashMap;
use std::sync::OnceLock;

/// Encode a string (ASCII letters and digits) to Morse code.
/// Returns the dot/dash sequence separated by spaces between symbols
/// and `/` between words. Unknown characters are silently skipped.
///
/// Examples:
/// - encode("SOS") -> "... --- ..."
/// - encode("HELLO") -> ".... . .-.. .-.. ---"
pub fn encode(s: &str) -> String {
    let table = forward_table();
    s.split_whitespace()
        .map(|word| {
            word.chars()
                .filter_map(|c| {
                    let upper = c.to_ascii_uppercase();
                    table.get(&upper).copied()
                })
                .collect::<Vec<&str>>()
                .join(" ")
        })
        .collect::<Vec<String>>()
        .join(" / ")
}

/// Decode a Morse code string back to ASCII. Uses `.` and `-` for
/// dit/dah; ` ` between symbols, `/` between words. Unknown sequences
/// become `?`.
///
/// Returns the decoded string with one `?` per unrecognized symbol.
pub fn decode(s: &str) -> String {
    let table = reverse_table();
    let mut out = String::new();
    for word in s.split('/') {
        for symbol in word.split_whitespace() {
            match table.get(symbol) {
                Some(&c) => out.push(c),
                None => out.push('?'),
            }
        }
        out.push(' ');
    }
    out.trim_end().to_string()
}

const CODE_TABLE: &[(char, &str)] = &[
    ('A', ".-"),
    ('B', "-..."),
    ('C', "-.-."),
    ('D', "-.."),
    ('E', "."),
    ('F', "..-."),
    ('G', "--."),
    ('H', "...."),
    ('I', ".."),
    ('J', ".---"),
    ('K', "-.-"),
    ('L', ".-.."),
    ('M', "--"),
    ('N', "-."),
    ('O', "---"),
    ('P', ".--."),
    ('Q', "--.-"),
    ('R', ".-."),
    ('S', "..."),
    ('T', "-"),
    ('U', "..-"),
    ('V', "...-"),
    ('W', ".--"),
    ('X', "-..-"),
    ('Y', "-.--"),
    ('Z', "--.."),
    ('0', "-----"),
    ('1', ".----"),
    ('2', "..---"),
    ('3', "...--"),
    ('4', "....-"),
    ('5', "....."),
    ('6', "-...."),
    ('7', "--..."),
    ('8', "---.."),
    ('9', "----."),
];

fn forward_table() -> &'static HashMap<char, &'static str> {
    static CELL: OnceLock<HashMap<char, &'static str>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut m = HashMap::new();
        for &(k, v) in CODE_TABLE {
            m.insert(k, v);
        }
        m
    })
}

fn reverse_table() -> &'static HashMap<&'static str, char> {
    static CELL: OnceLock<HashMap<&'static str, char>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut m = HashMap::new();
        for &(k, v) in CODE_TABLE {
            m.insert(v, k);
        }
        m
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_sos() {
        assert_eq!(encode("SOS"), "... --- ...");
    }

    #[test]
    fn encode_hello() {
        assert_eq!(encode("HELLO"), ".... . .-.. .-.. ---");
    }

    #[test]
    fn encode_digits() {
        assert_eq!(encode("12345"), ".---- ..--- ...-- ....- .....");
    }

    #[test]
    fn encode_multiple_words() {
        assert_eq!(encode("HELLO WORLD"), ".... . .-.. .-.. --- / .-- --- .-. .-.. -..");
    }

    #[test]
    fn encode_lowercase() {
        // Lower-case is supported via case-folding in the lookup table
        assert_eq!(encode("hello"), ".... . .-.. .-.. ---");
    }

    #[test]
    fn decode_basic() {
        assert_eq!(decode("... --- ..."), "SOS");
        assert_eq!(decode(".... . .-.. .-.. ---"), "HELLO");
    }

    #[test]
    fn decode_digits() {
        assert_eq!(decode(".---- ..--- ...-- ....- ....."), "12345");
    }

    #[test]
    fn decode_unknown_symbol_errors() {
        // Unrecognized symbols become '?'; words are split on '/'.
        // "..-.. / .--- / .-.-" → "..-.." unknown, ".---" = J, ".-.-" unknown
        assert_eq!(decode("..-.. / .--- / .-.-"), "? J ?");
    }

    #[test]
    fn decode_partial_word_errors() {
        // Within a single word (no '/'), unknown symbols also become '?'.
        // Symbols are joined without spaces; the space between words comes from '/'.
        assert_eq!(decode(".... . .-.."), "HEL");
        assert_eq!(decode(".... . .-.. .---."), "HEL?");
    }

    #[test]
    fn round_trip() {
        for s in ["SOS", "HELLO", "ABCDEFGHIJKLMNOPQRSTUVWXYZ", "0123456789"] {
            assert_eq!(decode(&encode(s)), s, "round-trip failed for {}", s);
        }
    }
}