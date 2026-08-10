//! Fuzz target: `psub_gateway::ini_parser`.
//!
//! Random bytes should never produce a panic. Keys may contain `=` or
//! look like section headers `[foo]`. The parser must reject malformed
//! input without panicking or getting stuck in a runaway loop.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = psub_gateway::ini_parser::parse(s);
    }
});
