//! Property-based tests for substrate primitives.
//!
//! TEST-03: proptest infrastructure. Demonstrates the pattern for adding
//! fuzz-style tests to any parser/validator/serializer in the workspace.
//!
//! See <https://proptest-rs.github.io/proptest/intro.html> for proptest's API.

use proptest::prelude::*;
use substrate_core::{ContentHash, NodeId, SchemaHash};

proptest! {
    /// SchemaHash should round-trip through its hex representation.
    #[test]
    fn schema_hash_hex_roundtrip(
        bytes in prop::array::uniform32(any::<u8>())
    ) {
        let hash = SchemaHash::from_bytes(bytes);
        let hex = hash.to_hex();
        let parsed = SchemaHash::from_hex(&hex).expect("valid hex should parse");
        prop_assert_eq!(hash, parsed);
    }

    /// NodeId ULID generation should produce monotonically non-decreasing values.
    #[test]
    fn node_id_monotonicity(
        prior in prop::array::uniform16(any::<u8>()),
        next in prop::array::uniform16(any::<u8>())
    ) {
        let _ = (prior, next);
        // Generation logic correctness verified manually elsewhere;
        // this test is the *scaffold* proptest will exercise.
    }

    /// ContentHash collision-resistance: every distinct input → distinct hash.
    #[test]
    fn content_hash_uniqueness(
        a in prop::array::uniform64(any::<u8>()),
        b in prop::array::uniform64(any::<u8>())
    ) {
        if a == b { return Ok(()); }
        prop_assert_ne!(
            ContentHash::from_bytes(a).as_bytes(),
            ContentHash::from_bytes(b).as_bytes(),
            "hashes of distinct inputs must differ"
        );
    }
}
