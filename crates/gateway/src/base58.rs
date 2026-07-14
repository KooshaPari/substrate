//! Base58 encoder + decoder.
//!
//! Base58 is a binary-to-text encoding used by Bitcoin addresses
//! (and many other systems) that avoids visually similar characters
//! (0/O, I/l). Uses the standard Bitcoin alphabet:
//! `123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz`.
//!
//! [`encode`] converts bytes to a base58 string. [`decode`] reverses.

const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Encode bytes to a base58 string. Leading zero bytes in the input
/// are preserved as leading `'1'` chars in the output (Bitcoin-style).
///
/// Examples:
/// - encode(b"") -> ""
/// - encode(b"\x00\x00hello") -> "11D6Lntxt"
pub fn encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    // Count leading zeros (each becomes a leading '1' in the output).
    let leading_zeros = data.iter().take_while(|&&b| b == 0).count();
    // Treat the non-zero suffix as a big-endian integer and repeatedly divide by 58
    // using a Vec<u8> digit buffer so we don't overflow on long inputs.
    let mut digits: Vec<u8> = Vec::new();
    let mut input: Vec<u8> = data[leading_zeros..].to_vec();
    while !input.is_empty() {
        let mut remainder: u32 = 0;
        let mut new_input: Vec<u8> = Vec::new();
        for &b in &input {
            let acc = remainder * 256 + b as u32;
            let q = acc / 58;
            remainder = acc % 58;
            if !new_input.is_empty() || q > 0 {
                new_input.push(q as u8);
            }
        }
        digits.push(remainder as u8);
        input = new_input;
    }
    digits.reverse();
    let mut out = String::with_capacity(leading_zeros + digits.len());
    for _ in 0..leading_zeros {
        out.push('1');
    }
    for &d in &digits {
        out.push(ALPHABET[d as usize] as char);
    }
    out
}

/// Decode a base58 string back to bytes. Returns `Err` on any character
/// outside the Bitcoin alphabet.
pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    // Leading '1' chars decode to leading zero bytes (Bitcoin convention).
    let leading_ones = s.chars().take_while(|&c| c == '1').count();

    // Translate chars to digit values 0..=57, error on anything outside the alphabet.
    let mut digits: Vec<u8> = Vec::with_capacity(s.len());
    for c in s.chars() {
        match ALPHABET.iter().position(|&a| a == c as u8) {
            Some(p) => digits.push(p as u8),
            None => return Err(format!("invalid character '{}'", c)),
        }
    }

    // Treat the digit sequence as a big-endian base-58 integer and accumulate it
    // into a little-endian byte buffer using repeated `* 58 + digit` arithmetic.
    // Each cell holds one byte (0..=255); carries roll over to a new cell.
    let mut n: Vec<u32> = vec![0];
    for &d in &digits {
        let mut carry: u32 = d as u32;
        for cell in n.iter_mut() {
            let acc = *cell * 58 + carry;
            *cell = acc & 0xff;
            carry = acc >> 8;
        }
        while carry > 0 {
            n.push(carry & 0xff);
            carry >>= 8;
        }
    }
    // Convert from little-endian cells (each u32 holds one byte) to a Vec<u8>.
    let le: Vec<u8> = n.iter().map(|&x| x as u8).collect();
    // Reverse to big-endian, then trim any leading zeros that came from padding
    // in the carry-propagation loop above.
    let be: Vec<u8> = le.iter().rev().copied().collect();
    let trimmed: &[u8] = if be.iter().all(|&b| b == 0) {
        &[]
    } else {
        let lead = be.iter().take_while(|&&b| b == 0).count();
        &be[lead..]
    };

    // Prepend leading zero bytes.
    let mut out = vec![0u8; leading_ones];
    out.extend_from_slice(trimmed);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_round_trip() {
        assert_eq!(encode(b""), "");
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn leading_zeros_preserved() {
        let encoded = encode(&[0, 0, 0, 1]);
        assert!(encoded.starts_with("111"));
        assert_eq!(decode(&encoded).unwrap(), vec![0, 0, 0, 1]);
    }

    #[test]
    fn encode_decode_round_trip() {
        for input in [
            vec![0u8],
            vec![0, 1],
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            (0..=255u8).collect::<Vec<u8>>(),
            b"hello world".to_vec(),
        ] {
            let encoded = encode(&input);
            let decoded = decode(&encoded).unwrap();
            assert_eq!(decoded, input, "round-trip failed for {input:?}");
        }
    }

    #[test]
    fn decode_bad_char_errors() {
        assert!(decode("0OIl").is_err()); // excluded chars
        assert!(decode("@@@@").is_err());
    }
}
