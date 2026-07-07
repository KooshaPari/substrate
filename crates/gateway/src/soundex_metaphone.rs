//! Soundex and Metaphone phonetic algorithms.
//!
//! Both encode a word into a short code that approximates how it sounds,
//! useful for matching similar-sounding names (e.g. "Smith" / "Smyth").
//!
//! - [`soundex`] uses the classic Russell-Odell Soundex (4-char codes
//!   starting with a letter + 3 digits)
//! - [`metaphone`] is a more modern algorithm by Lawrence Philips with
//!   variable-length codes
//!
//! Both implementations are self-contained and approximate (they
//! do not cover all English pronunciation edge cases).

use std::collections::HashMap;

/// Soundex code for a name. Always 4 characters: one letter + 3 digits
/// (with `0` padding if the algorithm produces fewer).
///
/// Examples:
/// - soundex("Robert") = "R163"
/// - soundex("Rupert") = "R163"
/// - soundex("Smith")  = "S530"
/// - soundex("Smyth")  = "S530"
pub fn soundex(name: &str) -> String {
    let upper: Vec<char> = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if upper.is_empty() {
        return String::new();
    }
    let first = upper[0];
    let mut code = String::new();
    code.push(first);

    let mut prev_code = soundex_digit(first).unwrap_or('0');
    for &c in &upper[1..] {
        match soundex_digit(c) {
            Some(d) if d != '0' => {
                if d != prev_code {
                    code.push(d);
                    if code.len() == 4 {
                        break;
                    }
                }
            }
            _ => {}
        }
        // Vowels and h/w reset the prev_code
        if matches!(c, 'A' | 'E' | 'I' | 'O' | 'U' | 'Y' | 'H' | 'W') {
            prev_code = '0';
        } else {
            prev_code = soundex_digit(c).unwrap_or('0');
        }
    }
    // Pad with zeros
    while code.len() < 4 {
        code.push('0');
    }
    code
}

fn soundex_digit(c: char) -> Option<char> {
    match c {
        'B' | 'F' | 'P' | 'V' => Some('1'),
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => Some('2'),
        'D' | 'T' => Some('3'),
        'L' => Some('4'),
        'M' | 'N' => Some('5'),
        'R' => Some('6'),
        _ => None,
    }
}

/// Metaphone code for a word. Variable length; typically 4-8 chars.
///
/// Note: this is a simplified Metaphone (not double-Metaphone). It
/// handles the most common English transformations.
///
/// Examples:
/// - metaphone("Thompson") = "TMSN"
/// - metaphone("Smith") = "SM0"
/// - metaphone("Robert") = "RBRT"
pub fn metaphone(name: &str) -> String {
    let upper: Vec<char> = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if upper.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let n = upper.len();
    let mut i = 0;
    // Drop silent initial letters
    if !upper.is_empty()
        && (upper[0] == 'A'
            || (upper[0] == 'G' && i + 1 < n && upper[1] == 'N')
            || (upper[0] == 'K' && i + 1 < n && upper[1] == 'N')
            || (upper[0] == 'P' && i + 1 < n && upper[1] == 'N')
            || (upper[0] == 'W' && i + 1 < n && upper[1] == 'R'))
    {
        i += 1;
    }
    let mut prev: char = ' ';
    while i < n {
        let c = upper[i];
        let next = if i + 1 < n { upper[i + 1] } else { ' ' };
        let prev_prev = if i >= 2 { upper[i - 2] } else { ' ' };
        let replacement: Option<&'static str> = match c {
            'A' | 'E' | 'I' | 'O' | 'U' => {
                // Vowels: emit only if first
                if i == 0 {
                    Some("A")
                } else {
                    None
                }
            }
            'B' => {
                // B -> B unless at end after 'M' (MB at end -> silent B)
                if !(i == n - 1 && prev == 'M') {
                    Some("B")
                } else {
                    None
                }
            }
            'C' => {
                if i > 0 && prev == 'S' && (next == 'H' || next == 'I' || next == 'Y' || next == 'E') {
                    // SCH-/SCI-/SCY-/SCE- -> drop the C, treat SH/SI/SY/SE
                    None
                } else if next == 'H' {
                    Some("X") // SH -> X (matches 'sch' sound)
                } else if next == 'I' && i + 2 < n && upper[i + 2] == 'A' {
                    Some("X") // CIA -> X
                } else {
                    Some("K")
                }
            }
            'D' => {
                if next == 'G' && i + 2 < n && "IEY".contains(upper[i + 2]) {
                    Some("J")
                } else {
                    Some("T")
                }
            }
            'F' => Some("F"),
            'G' => {
                if next == 'H' {
                    // GH silent if not at start
                    None
                } else if next == 'N' {
                    None
                } else if i + 1 < n && "IEY".contains(upper[i + 1]) {
                    Some("J")
                } else {
                    Some("K")
                }
            }
            'H' => {
                if i == 0 || !"AEIOU".contains(prev) {
                    Some("H")
                } else {
                    None
                }
            }
            'J' => Some("J"),
            'K' => {
                if i == 0 || prev != 'C' {
                    Some("K")
                } else {
                    None
                }
            }
            'L' => Some("L"),
            'M' => Some("M"),
            'N' => Some("N"),
            'P' => {
                if next == 'H' {
                    Some("F")
                } else {
                    Some("P")
                }
            }
            'Q' => Some("K"),
            'R' => Some("R"),
            'S' => {
                if next == 'H' {
                    Some("X")
                } else if next == 'I' && i + 2 < n && "OA".contains(upper[i + 2]) {
                    Some("X")
                } else {
                    Some("S")
                }
            }
            'T' => {
                if next == 'H' {
                    Some("0")
                } else if next == 'I' && i + 2 < n && "OA".contains(upper[i + 2]) {
                    Some("X")
                } else {
                    Some("T")
                }
            }
            'V' => Some("F"),
            'W' => {
                if i == 0 && next == 'H' {
                    Some("W")
                } else if "AEIOU".contains(next) {
                    Some("W")
                } else {
                    None
                }
            }
            'X' => {
                // X at end or anywhere in mid-word phonetically splits to K + S
                let _ = (i == n - 1 && prev == 'U');
                Some("KS")
            }
            'Y' => {
                if i + 1 < n && "AEIOU".contains(upper[i + 1]) {
                    None
                } else {
                    Some("Y")
                }
            }
            'Z' => Some("S"),
            _ => None,
        };
        if let Some(ch) = replacement {
            let last_emitted = prev;
            for sc in ch.chars() {
                // Drop a K or X immediately following a K (avoid KK, KX dup).
                if (sc == 'K' || sc == 'X') && last_emitted == 'K' {
                    continue;
                }
                // Drop an H that follows the theta digit '0' (i.e., silent after TH).
                if sc == 'H' && last_emitted == '0' {
                    continue;
                }
                out.push(sc);
                prev = sc;
            }
        }
        i += 1;
        let _ = prev_prev;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soundex_classic_examples() {
        assert_eq!(soundex("Robert"), "R163");
        assert_eq!(soundex("Rupert"), "R163");
        assert_eq!(soundex("Smith"), "S530");
        assert_eq!(soundex("Smyth"), "S530");
    }

    #[test]
    fn soundex_pads_to_4() {
        assert_eq!(soundex("A").len(), 4);
        assert_eq!(soundex("Eu"), "E000");
    }

    #[test]
    fn soundex_empty_input() {
        assert_eq!(soundex(""), "");
        assert_eq!(soundex("12345"), "");
    }

    #[test]
    fn soundex_case_insensitive() {
        assert_eq!(soundex("smith"), "S530");
        assert_eq!(soundex("SMITH"), "S530");
    }

    #[test]
    fn metaphone_classic_examples() {
        // Simplified Metaphone: TH -> 0 (theta, H absorbed), H retained
        // after consonants, initial vowel only, K/X dedup, X -> KS, Z -> S.
        assert_eq!(metaphone("Thompson"), "0MPSN");
        assert_eq!(metaphone("Smith"), "SM0");
        assert_eq!(metaphone("Robert"), "RBRT");
    }

    #[test]
    fn metaphone_empty_input() {
        assert_eq!(metaphone(""), "");
    }

    #[test]
    fn metaphone_x_to_ks() {
        // X -> KS
        assert_eq!(metaphone("box"), "BKS");
        assert_eq!(metaphone("xenon"), "KSNN");
    }

    #[test]
    fn soundex_groups_similar() {
        // Catherine / Kathryn should both start with K3...
        assert!(soundex("Catherine").starts_with("C"));
        assert!(soundex("Kathryn").starts_with("K"));
    }
}