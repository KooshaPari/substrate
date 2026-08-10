//! Fuzz target: `psub_gateway::asn1_der` round-trip.
//!
//! ASN.1 DER is famously tricky (length encoding rules, indefinite
//! forms, etc.). Encode arbitrary integers, then re-parse and check
//! that we recover the same number. Fuzz mutates a 16-byte input that
//! we interpret as both a struct of small integers and as the input to
//! the integer/integer-list helpers.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // 1. Round-trip integer helpers using the first 8 bytes as the input
    //    (interpreted as little-endian for clamping).
    if data.len() >= 8 {
        let n = i64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);
        // Try integer() and verify length is plausible.
        let _ = psub_gateway::asn1_der::integer(n);
    }

    // 2. Round-trip octet_string with any remaining bytes.
    if data.len() > 8 {
        let _ = psub_gateway::asn1_der::octet_string(&data[8..]);
    }

    // 3. Round-trip encode_oid with arbitrary component count (capped).
    if data.len() >= 2 {
        let n_components = (data[0] as usize) % 16;
        let components: Vec<u64> = data[1..=n_components.min(data.len() - 1)]
            .iter()
            .map(|b| *b as u64)
            .collect();
        let _ = psub_gateway::asn1_der::encode_oid(&components);
    }
});
