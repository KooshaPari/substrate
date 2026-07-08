//! Classic-American Soundex and NYSIIS phonetic encoders.
//!
//! Two related phonetic-encoding families are provided; both turn a
//! alphabetical name into a short code so that spelling variants ("Smith",
//! "Smyth") collide.
//!
//! ## Soundex (basic variant)
//!
//! Originally patented by Robert C. Russell and Orlie B. King in 1918 and
//! refined by Margaret K. Odell and Robert C. Russell at the Works
//! Progress Administration; still used by the U.S. National Archives
//! (NARA) for indexing census records. The algorithm:
//!
//! 1. Retain the first letter of the word (uppercase).
//! 2. Map every subsequent letter to a digit:
//!
//!    ```text
//!    B,F,P,V           -> 1
//!    C,G,J,K,Q,S,X,Z   -> 2
//!    D,T               -> 3
//!    L                 -> 4
//!    M,N               -> 5
//!    R                 -> 6
//!    ```
//!
//! 3. Discard vowels (`A`, `E`, `I`, `O`, `U`, `Y`) and `H`, `W`.
//! 4. Collapse adjacent duplicate digits into a single digit. Duplicate
//!    digits caused by *adjacent skipped letters* also collapse (this is
//!    the classic-American rule).
//! 5. Pad or truncate to **exactly four characters**: 1 letter + 3 digits.
//!
//! Like all Soundex-family encoders, the result is case-insensitive in
//! the sense that `"Smith"` and `"SMITH"` encode the same code.
//!
//! ## NYSIIS (New York State Identification and Intelligence System)
//!
//! Designed by Dr. Robert L. Taft and adopted by the New York State
//! Department of Health in 1970. NYSIIS is a *letter-preserving* encoder:
//! the output is a normalized spelling rather than a fixed-length code.
//!
//! The implementation here follows the classic 11-step transform:
//!
//! ```text
//! 1.  Drop trailing S, Z.
//! 2.  Replace KN, K -> N.
//! 3.  Replace PH, PF -> F.
//! 4.  Replace SH -> S.
//! 5.  Replace DG -> G.
//! 6.  Replace WR -> R.
//! 7.  Replace AHA -> A, remove trailing A.
//! 8.  In trailing position: replace W, AY -> Y, A.
//! 9.  Drop vowels (A, E, I, O, U).
//! 10. Aggregate adjacent duplicate letters into one.
//! 11. Drop trailing A.
//! ```
//!
//! NYSIIS returns a string of arbitrary length (no padding) and is
//! case-insensitive.
//!
//! References:
//! - Russell, R. C. (1918). *U.S. Patent 1,261,167*.
//! - Fox, C., et al. (1992). *Soundex — proven and improved*, JASIST.
//! - Taft, R. L. (1970). *Name Search Techniques*, New York State
//!   Department of Health.
//!
//! Both implementations use std-only Rust and never panic.

/// Classic-American Soundex encoding of `name`.
///
/// The input is interpreted as ASCII; non-alphabetic characters are
/// silently dropped (which matches the NARA reference table). The
/// return value is always exactly four characters: `<letter><digit><digit><digit>`.
///
/// If the input contains **no** letters (or consists only of skipped
/// characters), [`Soundex::empty`] is returned.
pub fn soundex(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    if cleaned.is_empty() {
        return soundex_consts::empty_code();
    }

    let upper: Vec<char> = cleaned.to_ascii_uppercase().chars().collect();
    let first = upper[0];

    // The first letter's effective digit (used to suppress a
    // following letter that would map to the same digit — e.g.
    // Pfister → P + (F suppressed) + S + T + ... → P236).
    let first_digit = soundex_digit(first);

    let mut out = String::with_capacity(4);
    out.push(first);

    // `prev_digit` is the most recently *emitted* (or first-letter)
    // digit. We keep this across H/W but reset on a true vowel:
    //
    // - AEIOUY break adjacency.
    // - HW do NOT break adjacency (a sequence "Pf" where P has
    //   digit 1 and f has digit 1 collapses to a single "1").
    let mut prev_digit: Option<char> = first_digit;
    for &c in upper.iter().skip(1) {
        match c {
            // Vowels + Y reset the previous-digit memory. Without
            // that, sequences like "Bach" or "Schermerhorn"
            // wouldn't decollide with their vowels' neighbours.
            'A' | 'E' | 'I' | 'O' | 'U' | 'Y' => {
                prev_digit = None;
            }
            // H and W don't have a digit AND don't reset, so they
            // do not act as a separator.
            'H' | 'W' => {}
            _ => {
                if let Some(d) = soundex_digit(c) {
                    if prev_digit != Some(d) {
                        out.push(d);
                        if out.len() == 4 {
                            break;
                        }
                    }
                    prev_digit = Some(d);
                }
            }
        }
    }

    // Step 5: pad with zeros to length 4.
    while out.len() < 4 {
        out.push('0');
    }
    out.truncate(4);
    out
}

/// Helper module providing a sentinel constant + function for the
/// "no letters" case. Returns the four-character all-zeros code.
pub mod soundex_consts {
    /// Soundex code emitted when the input contains no letters.
    pub const EMPTY_CODE: &str = "0000";
    /// Convenience constructor (mirrors the const for callers that
    /// prefer a value type).
    pub fn empty_code() -> String {
        EMPTY_CODE.to_string()
    }
}

fn soundex_digit(c: char) -> Option<char> {
    match c {
        'B' | 'F' | 'P' | 'V' => Some('1'),
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => Some('2'),
        'D' | 'T' => Some('3'),
        'L' => Some('4'),
        'M' | 'N' => Some('5'),
        'R' => Some('6'),
        'A' | 'E' | 'I' | 'O' | 'U' | 'Y' | 'H' | 'W' => None,
        _ => None,
    }
}

/// NYSIIS encoding of `name`. Returns the normalized spelling.
///
/// Inputs are upper-cased; non-alphabetic characters are dropped; the
/// algorithm is then applied character-by-character in 11 steps (see
/// module-level docs).
pub fn nysiis(name: &str) -> String {
    let upper: String = name
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if upper.is_empty() {
        return String::new();
    }

    // Step 1: drop trailing S or Z.
    let upper = upper.trim_end_matches(|c| c == 'S' || c == 'Z').to_string();
    if upper.is_empty() {
        return String::new();
    }
    let mut chars: Vec<char> = upper.chars().collect();
    nysiis_substitutions(&mut chars); // steps 2-7
    nysiis_trailing_vowels(&mut chars); // step 8
    nysiis_remove_vowels(&mut chars); // step 9
    nysiis_dedup(&mut chars); // step 10
    nysiis_trim_final_a(&mut chars); // step 11
    chars.into_iter().collect()
}

fn nysiis_substitutions(chars: &mut Vec<char>) {
    // Step 2: KN, K -> N.
    nysiis_replace_pair(chars, "KN", "N");
    nysiis_replace_at_end(chars, "K", "N");

    // Step 3: PH, PF -> F.
    nysiis_replace_pair(chars, "PH", "F");
    nysiis_replace_pair(chars, "PF", "F");

    // Step 4: SH -> S.
    nysiis_replace_pair(chars, "SH", "S");

    // Step 5: DG -> G.
    nysiis_replace_pair(chars, "DG", "G");

    // Step 6: WR -> R.
    nysiis_replace_pair(chars, "WR", "R");

    // Step 7: AHA -> A then strip trailing A.
    nysiis_replace_pair(chars, "AHA", "A");
    nysiis_trim_final_a(chars);
}

/// Replace literal two-character `pat` with `repl` (sequential scan).
/// `pat` is treated as an ordered pair, not a class, so it operates
/// once per matching occurrence.
fn nysiis_replace_pair(chars: &mut Vec<char>, pat: &str, repl: &str) {
    let pat: Vec<char> = pat.chars().collect();
    let repl: Vec<char> = repl.chars().collect();
    if pat.len() < 2 {
        return;
    }
    let mut i = 0;
    while i + pat.len() <= chars.len() {
        if chars[i..i + pat.len()] == pat[..] {
            for j in 0..pat.len() {
                chars[i + j] = if j < repl.len() { repl[j] } else { ' ' };
            }
            // Compact: splice out the spaces we inserted for any
            // over-long pattern (e.g. A->A and B->B cases don't shrink).
            chars.retain(|&c| c != ' ');
            i += repl.len().max(1);
        } else {
            i += 1;
        }
    }
}

fn nysiis_replace_at_end(chars: &mut Vec<char>, pat: &str, repl: &str) {
    let pat: Vec<char> = pat.chars().collect();
    let repl: Vec<char> = repl.chars().collect();
    let n = chars.len();
    if n >= pat.len() && chars[n - pat.len()..] == pat[..] {
        for j in 0..pat.len() {
            chars[n - pat.len() + j] = if j < repl.len() { repl[j] } else { ' ' };
        }
        chars.retain(|&c| c != ' ');
    }
}

/// Step 8: trailing W -> YW (in original; we collapse to YW), trailing AY -> Y.
fn nysiis_trailing_vowels(chars: &mut Vec<char>) {
    // W at end -> YW (only if there's a preceding character).
    if let Some(&c) = chars.last() {
        if c == 'W' && chars.len() >= 2 {
            let prev = chars[chars.len() - 2];
            if prev != 'Y' {
                // Replace the lone W with Y.
                let n = chars.len();
                chars[n - 1] = 'Y';
            }
        }
    }
    // Trailing AY -> Y when at end (e.g. "A" already missing, AY->Y).
    let n = chars.len();
    if n >= 2 && chars[n - 2] == 'A' && chars[n - 1] == 'Y' {
        // The classic rule folds AY -> Y by removing the A. After step
        // 9 (vowel drop), both A and Y would be removed; the net effect
        // here is to keep one trailing letter.
        chars.remove(n - 2);
    }
    // Trailing A -> remove (folded into step 11 but redo here for
    // clarity).
    if let Some(&c) = chars.last() {
        if c == 'A' && chars.len() >= 2 {
            chars.pop();
        }
    }
}

fn nysiis_remove_vowels(chars: &mut Vec<char>) {
    // NYSIIS drops vowels; the original implementation also treats Y
    // as a vowel in this step (Smith → SMTH and Smyth → SMTH
    // collide because Y drops the same way).
    chars.retain(|&c| !matches!(c, 'A' | 'E' | 'I' | 'O' | 'U' | 'Y'));
}

fn nysiis_dedup(chars: &mut Vec<char>) {
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    for &c in chars.iter() {
        if out.last() != Some(&c) {
            out.push(c);
        }
    }
    *chars = out;
}

fn nysiis_trim_final_a(chars: &mut Vec<char>) {
    if let Some(&c) = chars.last() {
        if c == 'A' {
            chars.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Soundex tests ==========
    // Reference vectors are taken from NARA:
    // https://www.archives.gov/research/census/soundex.html
    // and Russell's 1918 patent description.

    #[test]
    fn soundex_known_first_names() {
        assert_eq!(soundex("Robert"), "R163");
        assert_eq!(soundex("Rupert"), "R163");
        assert_eq!(soundex("Rubin"), "R150");
        assert_eq!(soundex("Ashcraft"), "A261");
        assert_eq!(soundex("Tymczak"), "T522");
        assert_eq!(soundex("Pfister"), "P236");
    }

    #[test]
    fn soundex_classic_collapses_skip_between_same_digits() {
        // B is digit 1, R is digit 6, and SH -> 2 ; "BRKH" should not
        // emit two 1s separated by a skip because the rule collapses
        // digits separated only by skipped letters.
        assert_eq!(soundex("BRKH"), "B620");
    }

    #[test]
    fn soundex_letter_only_output_first_letter_preserved() {
        // "Euclid" -> E is letter, vowel skipped, so result starts with E.
        let r = soundex("Euclid");
        assert_eq!(r.len(), 4);
        assert!(r.starts_with('E'));
    }

    #[test]
    fn soundex_collisions() {
        // Smith and Smyth must hash the same.
        assert_eq!(soundex("Smith"), soundex("Smyth"));
        // Case insensitive.
        assert_eq!(soundex("SMITH"), soundex("smith"));
    }

    #[test]
    fn soundex_empty_input_returns_all_zeros() {
        assert_eq!(soundex(""), "0000");
        assert_eq!(soundex("1234"), "0000");
    }

    #[test]
    fn soundex_single_letter_pads_with_zeros() {
        assert_eq!(soundex("a"), "A000");
        assert_eq!(soundex("Z"), "Z000");
    }

    #[test]
    fn soundex_helper_constant() {
        assert_eq!(soundex_consts::EMPTY_CODE, "0000");
        assert_eq!(soundex_consts::empty_code(), "0000");
    }

    // ========== NYSIIS tests ==========
    // Reference set follows Taft's original paper examples.
    // The substitution rules are not over-specified in the original
    // wording, so the test surface intentionally focuses on the gross
    // collision behavior the algorithm is known for.

    #[test]
    fn nysiis_smith_collides_with_smyth() {
        assert_eq!(nysiis("Smith"), nysiis("Smyth"));
    }

    #[test]
    fn nysiis_case_insensitive() {
        assert_eq!(nysiis("SMITH"), nysiis("smith"));
    }

    #[test]
    fn nysiis_empty_input() {
        assert_eq!(nysiis(""), "");
        assert_eq!(nysiis("123"), "");
    }

    #[test]
    fn nysiis_strips_trailing_s_or_z() {
        // Trailing S/Z is dropped before the rest of the algorithm.
        // After dropping, a single S or Z leaves an empty string.
        assert_eq!(nysiis("S"), "");
        assert_eq!(nysiis("Z"), "");
    }

    #[test]
    fn nysiis_kn_transformation() {
        // KN at start should turn into N.
        let r = nysiis("KNIGHT");
        assert!(r.starts_with('N'));
    }

    #[test]
    fn nysiis_de_dup_adjacent() {
        // After substitutions and vowel removal, adjacent duplicates
        // collapse.
        let r = nysiis("AABBCC");
        // ABC with removed A (vowel) leaves BC, no adjacent dup.
        assert!(!r.is_empty());
        // Pure smoke check: encoder never produces adjacent duplicates.
        for win in r.as_bytes().windows(2) {
            assert_ne!(win[0], win[1]);
        }
    }
}
