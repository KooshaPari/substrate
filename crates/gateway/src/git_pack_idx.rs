//! Minimal Git pack index v2 (the `.idx` companion to `.pack` files) reader.
//!
//! Wire format reference: `Documentation/gitformat-pack.txt` in git's source,
//! specifically the v2 index layout. We only parse; we don't generate.
//!
//! Layout (little-endian throughout):
//!
//! ```text
//!   0xFF 0x74 0x4F 0x63    4-byte magic
//!   "\0\0\0\0"             version (we require "00000002" as ASCII? no —
//!                           the spec says integer 2 in network order; on the
//!                           wire this is `0x00 0x00 0x00 0x02`)
//!   256 × u32              fan-out table
//!   N × 20-byte SHA-1      object names in lexicographic order
//!   N × u32                CRC32 of each object
//!   N × u32                pack offset (< 2^31) — large offsets are unused
//!   N × u64                large offsets (used when the corresponding
//!                           32-bit offset is the sentinel 0xFFFFFFFF)
//!   20-byte SHA-1          pack checksum (sha1 of the .pack file)
//!   20-byte SHA-1          idx checksum (sha1 of all preceding bytes)
//! ```
//!
//! This module does not verify checksums — it only parses shapes so callers
//! can map SHA → (offset, crc) without reaching for libgit2.

use std::convert::TryInto;

/// A parsed pack index. The fields mirror the on-disk tables so callers can
/// cross-reference them by index (`object_shas[i]` ↔ `crcs[i]` ↔
/// `offsets[i]` ↔ `large_offsets[i]`).
#[derive(Debug, Clone)]
pub struct PackIdx {
    /// 256-entry fanout table. `fanout[i]` is the number of objects whose
    /// first SHA byte is `≤ i`. The last entry is always the total count.
    pub fanout: [u32; 256],
    /// Object SHAs in lexicographic order.
    pub object_shas: Vec<[u8; 20]>,
    /// Per-object CRC32 (matches the order in `object_shas`).
    pub crcs: Vec<u32>,
    /// 32-bit pack offsets. The value `0xFFFF_FFFF` signals that the actual
    /// offset is in `large_offsets[i]` instead.
    pub offsets: Vec<u32>,
    /// 64-bit pack offsets used when the matching `offsets[i]` is the
    /// sentinel. Same length as `offsets` when populated; empty for indices
    /// without large offsets.
    pub large_offsets: Vec<u64>,
    /// SHA-1 of the corresponding `.pack` file.
    pub pack_checksum: [u8; 20],
    /// SHA-1 of all preceding bytes in the index file itself.
    pub idx_checksum: [u8; 20],
}

/// Sentinel value in `offsets` indicating a large offset lookup.
const LARGE_OFFSET_SENTINEL: u32 = 0xFFFF_FFFF;
/// Total byte count of the fixed-size prelude (magic + version + fanout).
const PRELUDE_LEN: usize = 4 + 4 + 256 * 4;
/// Fixed-size tables that scale with `N` (offsets + CRCs).
const FIXED_PER_OBJECT: usize = 4 + 4;

/// Parse a v2 pack index file from raw bytes.
///
/// Does not verify any SHA-1 checksums — the caller can do that with the
/// fields on [`PackIdx`].
pub fn parse(data: &[u8]) -> Result<PackIdx, String> {
    if data.len() < PRELUDE_LEN {
        return Err(format!(
            "pack index too short: have {} bytes, need at least {}",
            data.len(),
            PRELUDE_LEN
        ));
    }
    // Magic.
    if &data[..4] != b"\xfftOc" {
        return Err(format!("pack index magic mismatch: got {:?}", &data[..4]));
    }
    // Version: 4-byte integer in network byte order. Spec mandates 2.
    let version = u32::from_be_bytes(data[4..8].try_into().unwrap());
    if version != 2 {
        return Err(format!("unsupported pack index version: {}", version));
    }

    // Fanout.
    let mut fanout = [0u32; 256];
    for (i, slot) in fanout.iter_mut().enumerate() {
        let off = 8 + i * 4;
        *slot = u32::from_be_bytes(data[off..off + 4].try_into().unwrap());
    }
    let n = fanout[255] as usize;

    let total_fixed = n * 20 + n * FIXED_PER_OBJECT;
    // SHA table.
    let sha_start = PRELUDE_LEN;
    let sha_end = sha_start + n * 20;
    let crc_start = sha_end;
    let crc_end = crc_start + n * 4;
    let off_start = crc_end;
    let off_end = off_start + n * 4;

    if data.len() < off_end + 40 {
        return Err(format!(
            "pack index truncated: need {} bytes for {} objects (without large offsets), have {}",
            off_end + 40,
            n,
            data.len()
        ));
    }
    // `total_fixed` retained for future call sites; kept here as documentation.
    let _ = total_fixed;

    let mut object_shas = Vec::with_capacity(n);
    for i in 0..n {
        let start = sha_start + i * 20;
        let mut sha = [0u8; 20];
        sha.copy_from_slice(&data[start..start + 20]);
        object_shas.push(sha);
    }
    let mut crcs = Vec::with_capacity(n);
    for i in 0..n {
        let start = crc_start + i * 4;
        crcs.push(u32::from_be_bytes(
            data[start..start + 4].try_into().unwrap(),
        ));
    }
    let mut offsets = Vec::with_capacity(n);
    for i in 0..n {
        let start = off_start + i * 4;
        offsets.push(u32::from_be_bytes(
            data[start..start + 4].try_into().unwrap(),
        ));
    }

    // Large offsets: count how many entries have the sentinel, and either
    // expect a contiguous run of `count × u64` BE values, or none at all.
    let large_count = offsets
        .iter()
        .filter(|o| **o == LARGE_OFFSET_SENTINEL)
        .count();
    let large_offsets_end = off_end + large_count * 8;
    let mut large_offsets = Vec::with_capacity(large_count);
    for i in 0..large_count {
        let start = off_end + i * 8;
        large_offsets.push(u64::from_be_bytes(
            data[start..start + 8].try_into().unwrap(),
        ));
    }

    if data.len() < large_offsets_end + 40 {
        return Err(format!(
            "pack index truncated before trailing checksums: need {}, have {}",
            large_offsets_end + 40,
            data.len()
        ));
    }

    // Pack checksum and idx checksum.
    let pack_off = large_offsets_end;
    let idx_off = pack_off + 20;
    let mut pack_checksum = [0u8; 20];
    let mut idx_checksum = [0u8; 20];
    pack_checksum.copy_from_slice(&data[pack_off..pack_off + 20]);
    idx_checksum.copy_from_slice(&data[idx_off..idx_off + 20]);

    let expected = data.len() - idx_off - 20;
    if expected != 0 {
        // Trailing garbage is allowed in theory (git's loose parser ignores
        // it) but we fail loud rather than silently truncate.
        return Err(format!(
            "pack index has trailing bytes: {} bytes after idx checksum",
            expected
        ));
    }

    Ok(PackIdx {
        fanout,
        object_shas,
        crcs,
        offsets,
        large_offsets,
        pack_checksum,
        idx_checksum,
    })
}

/// Look up the pack offset of `sha` in the index. Returns `None` if the SHA
/// is not present.
pub fn lookup(idx: &PackIdx, sha: &[u8; 20]) -> Option<u64> {
    // Binary search by first byte via the fanout table, then linear scan
    // inside the matching bucket. For a production index this would be a
    // full binary search; the fanout prefix speeds things up only on
    // unindexed SHAs.
    let first = sha[0] as usize;
    let lo = if first == 0 {
        0
    } else {
        idx.fanout[first - 1] as usize
    };
    let hi = idx.fanout[first] as usize;
    for i in lo..hi {
        if &idx.object_shas[i] == sha {
            if idx.offsets[i] == LARGE_OFFSET_SENTINEL {
                return idx.large_offsets.get(i).copied();
            }
            return Some(idx.offsets[i] as u64);
        }
    }
    None
}

/// Builder helpers — exposed so tests can craft tiny fixtures without
/// re-implementing the wire format inline.
pub mod builder {
    use super::*;

    /// Build a minimal v2 pack index with `objects.len()` entries.
    ///
    /// All offsets are written as plain `u32`s (no large-offset sentinel).
    /// Checksums are populated with `0xAA`/`0xBB` placeholders so tests
    /// don't need a SHA-1 implementation; if you want to validate them, run
    /// the real `sha-1` over the bytes you assembled.
    pub fn build(objects: &[([u8; 20], u32)]) -> Vec<u8> {
        assert!(
            objects.len() <= u32::MAX as usize,
            "too many objects for u32 fanout"
        );
        // Sort by SHA lexicographically; pack indices are always sorted.
        let mut objs = objects.to_vec();
        objs.sort_by(|a, b| a.0.cmp(&b.0));

        let mut out = Vec::with_capacity(PRELUDE_LEN + objs.len() * (20 + 4 + 4) + 40);
        // Magic.
        out.extend_from_slice(b"\xfftOc");
        // Version 2, big-endian.
        out.extend_from_slice(&2u32.to_be_bytes());

        // Build fanout.
        let mut fanout = [0u32; 256];
        for (sha, _) in &objs {
            let b = sha[0] as usize;
            for slot in fanout.iter_mut().skip(b) {
                *slot += 1;
            }
        }
        for v in fanout.iter() {
            out.extend_from_slice(&v.to_be_bytes());
        }

        // SHA, CRC, offset (CRC comes from caller; here we emit zeros).
        for (sha, _) in &objs {
            out.extend_from_slice(sha);
        }
        for _ in &objs {
            out.extend_from_slice(&0u32.to_be_bytes());
        }
        for (_, off) in &objs {
            out.extend_from_slice(&off.to_be_bytes());
        }

        // Pad a fake trailing large-offset table (empty), then checksums.
        out.extend_from_slice(&[0xAA; 20]); // pack checksum placeholder
        out.extend_from_slice(&[0xBB; 20]); // idx checksum placeholder
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use builder::build;

    fn sha(byte: u8) -> [u8; 20] {
        let mut s = [0u8; 20];
        s[0] = byte;
        s
    }

    #[test]
    fn parses_minimal_handcrafted_index() {
        let a = sha(0x01);
        let b = sha(0x42);
        let bytes = build(&[(a, 1024), (b, 2048)]);
        let idx = parse(&bytes).expect("parse");
        assert_eq!(idx.object_shas.len(), 2);
        // The a SHA's first byte is 0x01, so it is counted under buckets >= 0x01.
        // The b SHA's first byte is 0x42 — every bucket >= 0x42 counts both.
        assert_eq!(idx.fanout[0], 0, "no SHA has first byte <= 0x00");
        assert_eq!(idx.fanout[0x01], 1, "a is included from 0x01 onward");
        assert_eq!(idx.fanout[0x42], 2, "both included from 0x42 onward");
        assert_eq!(idx.fanout[255], 2, "fanout[255] is the total");
        assert_eq!(idx.offsets[0], 1024);
        assert_eq!(idx.offsets[1], 2048);
    }

    #[test]
    fn round_trip_lookup_returns_offset() {
        let s1 = sha(0x10);
        let s2 = sha(0x20);
        let s3 = sha(0x30);
        let bytes = build(&[(s1, 100), (s2, 200), (s3, 300)]);
        let idx = parse(&bytes).expect("parse");
        assert_eq!(lookup(&idx, &s1), Some(100));
        assert_eq!(lookup(&idx, &s2), Some(200));
        assert_eq!(lookup(&idx, &s3), Some(300));
    }

    #[test]
    fn lookup_returns_none_for_missing_sha() {
        let bytes = build(&[(sha(0x05), 42)]);
        let idx = parse(&bytes).expect("parse");
        assert_eq!(lookup(&idx, &sha(0x99)), None);
    }

    #[test]
    fn fanout_table_monotonically_nondecreasing() {
        let bytes = build(&[
            (sha(0x01), 1),
            (sha(0x05), 2),
            (sha(0x80), 3),
            (sha(0xFE), 4),
        ]);
        let idx = parse(&bytes).expect("parse");
        for i in 1..256 {
            assert!(
                idx.fanout[i] >= idx.fanout[i - 1],
                "fanout not monotone at {}: {} < {}",
                i,
                idx.fanout[i],
                idx.fanout[i - 1]
            );
        }
        assert_eq!(idx.fanout[255], 4);
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut bytes = build(&[(sha(0x01), 1)]);
        bytes[0] = 0x00;
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn rejects_wrong_version() {
        let mut bytes = build(&[(sha(0x01), 1)]);
        // Overwrite version field with `3`.
        bytes[4] = 0;
        bytes[5] = 0;
        bytes[6] = 0;
        bytes[7] = 3;
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn empty_index_parses_cleanly() {
        // Manually craft an empty index: magic + version + zero fanout +
        // trailing checksums. Length = 4 + 4 + 256*4 + 40 = 1068.
        let mut bytes = Vec::with_capacity(PRELUDE_LEN + 40);
        bytes.extend_from_slice(b"\xfftOc");
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 256 * 4]);
        bytes.extend_from_slice(&[0x11u8; 20]); // pack
        bytes.extend_from_slice(&[0x22; 20]); // idx
        let idx = parse(&bytes).expect("parse");
        assert_eq!(idx.object_shas.len(), 0);
        assert_eq!(idx.crcs.len(), 0);
        assert_eq!(idx.offsets.len(), 0);
        assert!(idx.large_offsets.is_empty());
        assert_eq!(idx.fanout[0], 0);
        assert_eq!(idx.fanout[255], 0);
    }

    #[test]
    fn multi_object_lookup_iterates_correctly() {
        let shas: Vec<[u8; 20]> = (0u8..16).map(sha).collect();
        let pairs: Vec<([u8; 20], u32)> = shas
            .iter()
            .enumerate()
            .map(|(i, s)| (*s, (i as u32) * 8))
            .collect();
        let bytes = build(&pairs);
        let idx = parse(&bytes).expect("parse");
        assert_eq!(idx.object_shas.len(), 16);
        for (i, s) in shas.iter().enumerate() {
            assert_eq!(lookup(&idx, s), Some((i as u64) * 8));
        }
        assert_eq!(lookup(&idx, &sha(0xFF)), None);
    }
}
