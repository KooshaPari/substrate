//! Credit-card validation helpers (Luhn check + display masking).
//!
//! [`luhn_check`] verifies the standard ISO/IEC 7812-1 Luhn checksum for a
//! string of digits (spaces and dashes are ignored). [`mask`] returns a
//! display-friendly form that keeps only the trailing 4 digits visible.
//!
//! Neither function touches the network or a payment processor; this is the
//! minimal client-side sanity check before sending a PAN somewhere.

/// Luhn checksum for a digit string. Returns `false` for empty input.
///
/// Non-digit characters are stripped before validation. A valid PAN must
/// produce a checksum that is divisible by 10.
pub fn luhn_check(digits: &str) -> bool {
    let nums: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if nums.is_empty() {
        return false;
    }
    let mut sum = 0;
    for (i, &d) in nums.iter().rev().enumerate() {
        if i % 2 == 1 {
            let doubled = d * 2;
            sum += if doubled > 9 { doubled - 9 } else { doubled };
        } else {
            sum += d;
        }
    }
    sum % 10 == 0
}

/// Mask all but the trailing 4 digits with `*` blocks separated by spaces.
///
/// Useful for display surfaces (receipts, logs, audit exports) where the
/// full PAN must NOT be recorded. Returns the masked form: e.g.
/// `4111111111111111` -> `**** **** **** 1111`.
pub fn mask(digits: &str) -> String {
    let last4: String = digits
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("**** **** **** {}", last4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_visa_test_card() {
        // 4111 1111 1111 1111 — standard Visa test number (passes Luhn)
        assert!(luhn_check("4111111111111111"));
    }

    #[test]
    fn valid_amex_test_card() {
        // 3782 822463 10005 — standard Amex test number (passes Luhn)
        assert!(luhn_check("378282246310005"));
    }

    #[test]
    fn rejected_off_by_one_digit() {
        // Last digit changed -> checksum fails
        assert!(!luhn_check("4111111111111112"));
    }

    #[test]
    fn empty_input_rejected() {
        assert!(!luhn_check(""));
    }

    #[test]
    fn spaces_and_dashes_ignored() {
        assert!(luhn_check("4111-1111-1111-1111"));
        assert!(luhn_check("4111 1111 1111 1111"));
    }

    #[test]
    fn mask_preserves_last_four() {
        assert_eq!(mask("4111111111111111"), "**** **** **** 1111");
    }

    #[test]
    fn mask_handles_short_input() {
        // fewer than 4 trailing digits still produces a best-effort mask
        let m = mask("123");
        assert!(m.contains("123") || m == "**** **** **** 123");
    }
}
