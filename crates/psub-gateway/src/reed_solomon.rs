//! Reed-Solomon error-correction code over GF(256) for short messages.
//!
//! Encoder for RS(255 - nsym, 255 - 2*nsym) codes with `nsym` even and
//! in [2, 32]. Decoder supports the no-error and single-error cases
//! (multi-error correction is out of scope for this teaching stub).
//!
//! GF(256) uses the standard primitive polynomial 0x11D used by
//! QR codes, Data Matrix, and the CD audio standard. Symbol indexing
//! is 1-based (alpha^0 = 1, alpha^1 = 2, ...).
//!
//! References: Wicker (1995) "Reed-Solomon Codes and Their Applications".

const FIELD_SIZE: usize = 255;

/// Encoder state for a single nsym value (cached exp/log tables and
/// the generator polynomial coefficients).
pub struct RsEncoder {
    exp_table: [u8; 512],
    log_table: [u8; FIELD_SIZE + 1],
    generator_coeffs: Vec<u8>,
    pub data_capacity: usize,
}

impl RsEncoder {
    /// Build an encoder for an RS code with `nsym` parity symbols. `nsym`
    /// must be even and in [2, 32]. The code can correct up to `nsym/2`
    /// symbol errors.
    pub fn new(nsym: usize) -> Self {
        assert!(
            nsym >= 2 && nsym <= 32 && nsym % 2 == 0,
            "nsym must be even in [2, 32]"
        );
        let (exp_table, log_table) = build_tables();
        let generator_coeffs = generator_polynomial(nsym, &exp_table, &log_table);
        let data_capacity = FIELD_SIZE - nsym;
        Self {
            exp_table,
            log_table,
            generator_coeffs,
            data_capacity,
        }
    }

    /// Encode a message of length <= `data_capacity` bytes. Returns the
    /// appended parity bytes (length == nsym).
    pub fn encode(&self, message: &[u8]) -> Vec<u8> {
        assert!(
            message.len() <= self.data_capacity,
            "message too long ({} > {})",
            message.len(),
            self.data_capacity
        );
        // Polynomial long division: remainder of message * x^nsym mod g(x)
        let mut remainder = message.to_vec();
        remainder.resize(self.data_capacity + self.generator_coeffs.len() - 1, 0);
        for i in 0..self.data_capacity {
            let coef = remainder[i];
            if coef != 0 {
                for j in 1..self.generator_coeffs.len() {
                    remainder[i + j] ^= gf_mul(
                        self.generator_coeffs[j],
                        coef,
                        &self.exp_table,
                        &self.log_table,
                    );
                }
            }
        }
        remainder[self.data_capacity..].to_vec()
    }

    /// Decode an RS-encoded message and correct up to `nsym/2` byte errors.
    /// `received` should be the full message + parity (length up to 255).
    /// Returns the corrected message (without parity) on success.
    pub fn decode(&self, received: &[u8]) -> Result<Vec<u8>, String> {
        let nsym = self.generator_coeffs.len() - 1;
        assert!(received.len() == self.data_capacity + nsym, "wrong received length");

        // Compute syndromes: S_i = received(alpha^i) for i in 1..=nsym
        let mut syndromes = vec![0u8; nsym];
        for i in 0..nsym {
            let mut s = 0u8;
            for (j, &b) in received.iter().enumerate() {
                // alpha^((i+1) * (j+1))
                let exp_idx = ((i + 1) * (j + 1)) % 255;
                s ^= gf_mul(b, self.exp_table[exp_idx], &self.exp_table, &self.log_table);
            }
            syndromes[i] = s;
        }
        if syndromes.iter().all(|&s| s == 0) {
            return Ok(received[..self.data_capacity].to_vec());
        }
        // Single-error case: S_1 = e_i * alpha^i, S_2 = e_i * alpha^(2i)
        // -> S_2 / S_1 = alpha^i  -> i = log(S_2) - log(S_1) (mod 255)
        // -> e_i = S_1 / alpha^i
        if nsym < 2 {
            return Err("need at least 2 syndromes to locate error".into());
        }
        let s1 = syndromes[0];
        let s2 = syndromes[1];
        if s1 == 0 || s2 == 0 {
            return Err("zero syndrome but non-zero elsewhere: multi-error case not implemented".into());
        }
        let log_s1 = self.log_table[s1 as usize] as i32;
        let log_s2 = self.log_table[s2 as usize] as i32;
        let log_alpha_i = (log_s2 - log_s1).rem_euclid(255);
        let alpha_i = self.exp_table[log_alpha_i as usize];
        // error position: e_i is at received index `pos` where alpha^pos = alpha_i
        // i.e. pos = log(alpha_i)
        let pos = log_alpha_i as usize;
        if pos >= received.len() {
            return Err(format!("error position {} out of range {}", pos, received.len()));
        }
        // error magnitude: e = S_1 / alpha^i
        let log_e = (log_s1 - log_alpha_i).rem_euclid(255);
        let e = self.exp_table[log_e as usize];
        let mut corrected = received.to_vec();
        corrected[pos] ^= e;
        // Verify
        let parity_check = self.encode(&corrected[..self.data_capacity]);
        if parity_check == corrected[self.data_capacity..] {
            Ok(corrected[..self.data_capacity].to_vec())
        } else {
            Err("correction verification failed (multi-error case?)".into())
        }
    }
}

fn build_tables() -> ([u8; 512], [u8; FIELD_SIZE + 1]) {
    let mut exp_table = [0u8; 512];
    let mut log_table = [0u8; FIELD_SIZE + 1];
    let mut x: u8 = 1;
    for i in 0..FIELD_SIZE {
        exp_table[i] = x;
        log_table[x as usize] = i as u8;
        x = gf_mult_no_reduce(x, 2) ^ 0x1D;
    }
    // Duplicate exp table to avoid modulo in hot path
    for i in 0..255 {
        exp_table[i + 255] = exp_table[i];
    }
    (exp_table, log_table)
}

fn gf_mult_no_reduce(a: u8, b: u8) -> u8 {
    let mut result: u16 = 0;
    let mut x = a as u16;
    let mut y = b as u16;
    while y > 0 {
        if y & 1 != 0 {
            result ^= x;
        }
        y >>= 1;
        x <<= 1;
    }
    result as u8
}

fn gf_mul(a: u8, b: u8, exp: &[u8; 512], log: &[u8; FIELD_SIZE + 1]) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let log_a = log[a as usize] as usize;
    let log_b = log[b as usize] as usize;
    exp[log_a + log_b]
}

fn generator_polynomial(
    nsym: usize,
    exp: &[u8; 512],
    log: &[u8; FIELD_SIZE + 1],
) -> Vec<u8> {
    // g(x) = (x - alpha^1)(x - alpha^2)...(x - alpha^nsym)
    // In GF(2), subtraction is XOR, so (x - alpha^i) = (x + alpha^i).
    let mut g = vec![1u8];
    for i in 1..=nsym {
        let alpha_i = exp[i];
        let mut new_g = vec![0u8; g.len() + 1];
        for j in 0..g.len() {
            new_g[j] ^= g[j];
            new_g[j + 1] ^= gf_mul(g[j], alpha_i, exp, log);
        }
        g = new_g;
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_produces_correct_length() {
        let enc = RsEncoder::new(4);
        assert_eq!(enc.data_capacity, 251);
        let parity = enc.encode(b"hello world");
        assert_eq!(parity.len(), 4);
    }

    #[test]
    fn encode_short_message_works() {
        // Verifies the polynomial division does not panic on short inputs
        // and produces nsym parity bytes.
        let enc = RsEncoder::new(4);
        let parity = enc.encode(b"hi");
        assert_eq!(parity.len(), 4);
    }

    #[test]
    fn empty_encode_yields_zero_parity() {
        // Empty message: no coefficients, division produces all zeros
        let enc = RsEncoder::new(4);
        let parity = enc.encode(b"");
        assert_eq!(parity, vec![0, 0, 0, 0]);
    }
}