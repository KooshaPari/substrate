//! Morse code encoder/decoder.
//!
//! Standard ITU Morse code table for ASCII letters and digits. Useful
//! for shortwave radio, ham radio practice, and CTF-style encoding
//! puzzles. Punctuation is intentionally NOT supported — callers should
//! preprocess or encode punctuation separately.
//!
//! Reference: ITU-T Recommendation M.1677 (2009).

use std::collections::HashMap;

// Case-insensitive table lookup. The static table mixes upper-case ASCII
// keys with their Morse representation, so callers like `encode("hello")`
// must still match. CODE_TABLE is declared as a slice below this function.
fn lookup(c: char) -> Option<&'static str> {
    let folded = c.to_ascii_uppercase();
    CODE_TABLE
        .iter()
        .find(|(k, _)| *k == folded)
        .map(|(_, v)| *v)
}

/// Encode a string (ASCII letters and digits) to Morse code.
/// Returns the dot/dash sequence separated by spaces between symbols
/// and `/` between words. Unknown characters are silently skipped.
///
/// Examples:
/// - encode("SOS") -> "... --- ..."
/// - encode("HELLO") -> ".... . .-.. .-.. ---"
pub fn encode(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            word.chars()
                .filter_map(lookup)
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
    let mut table: HashMap<&str, char> = HashMap::new();
    for (k, v) in CODE_TABLE.iter() {
        table.insert(*v, *k);
    }
    let mut out = String::new();
    for word in s.split('/') {
        for symbol in word.split_whitespace() {
            match table.get(symbol) {
                Some(&c) => out.push(c),
                None => out.push('?'),
            }
            // Insert a placeholder between symbols that came from
            // the same word but happened to not decode — already
            // covered by pushing the literal char above.
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
        assert_eq!(decode("..-.. / .--- / .-.-"), "FU?");
        // Note: "-.-.-" isn't valid; ends up as `?`
    }

    #[test]
    fn round_trip() {
        for s in ["SOS", "HELLO", "ABCDEFGHIJKLMNOPQRSTUVWXYZ", "0123456789"] {
            assert_eq!(decode(&encode(s)), s, "round-trip failed for {}", s);
        }
    }
}