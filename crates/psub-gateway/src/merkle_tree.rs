//! Merkle tree over 32-byte SHA-256 digests.
//!
//! A Merkle tree is a binary hash tree where each internal node is the hash of
//! the concatenation of its two children. The root commit to the entire
//! contents of the tree. Inclusion proofs ("membership") are the sibling
//! hashes along the path from a leaf to the root, requiring O(log n) hashes
//! for verification. This module builds trees from leaf digests, computes the
//! root, and verifies membership proofs.
//!
//! Reference: Ralph C. Merkle, "A Certified Digital Signature" (1979);
//! RFC 9162, "Certificate Transparency Version 2.0", §2.1 (Merkle Tree Hash).
//!
//! Hashing uses the in-crate [`crate::sha256`] (FIPS 180-4 SHA-256).
//!
//! Duplicate adjacent leaves are concatenated then hashed with a `0x01`
//! domain-separation byte per RFC 9162 §2.1. The empty tree's root is
//! 32 zero bytes (a well-defined sentinel).

use crate::sha256;

/// Domain-separation bytes per RFC 9162 §2.1.
const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;

/// Compute the Merkle tree root hash from a slice of 32-byte leaf digests.
///
/// Returns `[0u8; 32]` for an empty input (a deterministic sentinel that
/// callers can compare against). If `leaves.len()` is not a power of two, the
/// last leaf is duplicated upward until the level is square.
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    if leaves.len() == 1 {
        return leaves[0];
    }
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            let mut buf = [0u8; 1 + 32 + 32];
            buf[0] = NODE_PREFIX;
            buf[1..33].copy_from_slice(&left);
            buf[33..65].copy_from_slice(&right);
            let h = sha256::hash(&buf);
            next.push(h);
            i += 2;
        }
        level = next;
    }
    level[0]
}

/// Compute a Merkle root over arbitrary-length chunks by first hashing each
/// chunk with [`sha256::hash`], then building a tree.
pub fn merkle_root_chunks(chunks: &[&[u8]]) -> [u8; 32] {
    if chunks.is_empty() {
        return [0u8; 32];
    }
    let leaves: Vec<[u8; 32]> = chunks
        .iter()
        .map(|c| {
            let mut buf = [0u8; 1 + 32];
            buf[0] = LEAF_PREFIX;
            let h = sha256::hash(c);
            buf[1..33].copy_from_slice(&h);
            sha256::hash(&buf)
        })
        .collect();
    merkle_root(&leaves)
}

/// A Merkle inclusion proof for the leaf at `index`.
///
/// `proof` lists sibling hashes from the leaf's sibling up to the root's
/// sibling (or child), bottom-up. `direction[i]` is `true` if the sibling at
/// `proof[i]` is on the right (i.e. the current node is the left child) and
/// `false` if the sibling is on the left.
#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub index: usize,
    pub proof: Vec<[u8; 32]>,
    pub direction: Vec<bool>,
}

/// Build the inclusion proof for the leaf at position `index`.
///
/// `leaves` is the original list of leaf digests. Returns `None` if `index`
/// is out of range.
pub fn build_proof(leaves: &[[u8; 32]], index: usize) -> Option<MerkleProof> {
    if index >= leaves.len() {
        return None;
    }
    let mut proof = Vec::new();
    let mut direction = Vec::new();
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    let mut idx = index;
    while level.len() > 1 {
        let sibling_idx = idx ^ 1;
        let sibling = if sibling_idx < level.len() {
            level[sibling_idx]
        } else {
            // Odd-level duplication: sibling == self.
            level[idx]
        };
        proof.push(sibling);
        direction.push(sibling_idx > idx); // sibling on the right?
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            let mut buf = [0u8; 1 + 32 + 32];
            buf[0] = NODE_PREFIX;
            buf[1..33].copy_from_slice(&left);
            buf[33..65].copy_from_slice(&right);
            let h = sha256::hash(&buf);
            next.push(h);
            i += 2;
        }
        level = next;
        idx >>= 1;
    }
    Some(MerkleProof {
        index,
        proof,
        direction,
    })
}

/// Verify that `leaf` is included at `proof.index` in a tree with `root`.
///
/// Returns `true` iff replaying the proof reconstructs `root`.
pub fn verify_proof(root: &[u8; 32], leaf: &[u8; 32], proof: &MerkleProof) -> bool {
    let mut current = *leaf;
    for (sibling, &on_right) in proof.proof.iter().zip(proof.direction.iter()) {
        let mut buf = [0u8; 1 + 32 + 32];
        buf[0] = NODE_PREFIX;
        if on_right {
            buf[1..33].copy_from_slice(&current);
            buf[33..65].copy_from_slice(sibling);
        } else {
            buf[1..33].copy_from_slice(sibling);
            buf[33..65].copy_from_slice(&current);
        }
        current = sha256::hash(&buf);
    }
    &current == root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[0] = b;
        out
    }

    #[test]
    fn empty_tree_root_is_zero() {
        assert_eq!(merkle_root(&[]), [0u8; 32]);
    }

    #[test]
    fn single_leaf_is_itself() {
        let leaf = h(0xab);
        assert_eq!(merkle_root(&[leaf]), leaf);
    }

    #[test]
    fn two_leaves_differ_from_each() {
        let a = merkle_root(&[h(0x01)]);
        let ab = merkle_root(&[h(0x01), h(0x02)]);
        assert_ne!(a, ab);
    }

    #[test]
    fn root_changes_when_any_leaf_changes() {
        let r1 = merkle_root(&[h(0x01), h(0x02), h(0x03), h(0x04)]);
        let r2 = merkle_root(&[h(0x01), h(0x02), h(0x03), h(0x05)]);
        assert_ne!(r1, r2);
    }

    #[test]
    fn deterministic_root() {
        let leaves = [h(0x10), h(0x20), h(0x30), h(0x40)];
        assert_eq!(merkle_root(&leaves), merkle_root(&leaves));
    }

    #[test]
    fn verify_proof_round_trip() {
        let leaves: Vec<[u8; 32]> = (0..8u8).map(|i| h(i)).collect();
        let root = merkle_root(&leaves);
        for i in 0..leaves.len() {
            let proof = build_proof(&leaves, i).unwrap();
            assert!(verify_proof(&root, &leaves[i], &proof));
        }
    }

    #[test]
    fn verify_proof_rejects_tampered_leaf() {
        let leaves: Vec<[u8; 32]> = (0..4u8).map(|i| h(i)).collect();
        let root = merkle_root(&leaves);
        let proof = build_proof(&leaves, 1).unwrap();
        let tampered = h(0xcc);
        assert!(!verify_proof(&root, &tampered, &proof));
    }

    #[test]
    fn verify_proof_rejects_tampered_proof() {
        let leaves: Vec<[u8; 32]> = (0..4u8).map(|i| h(i)).collect();
        let root = merkle_root(&leaves);
        let mut proof = build_proof(&leaves, 0).unwrap();
        // Flip one sibling hash.
        proof.proof[0][0] ^= 0xff;
        assert!(!verify_proof(&root, &leaves[0], &proof));
    }

    #[test]
    fn proof_index_out_of_range_is_none() {
        let leaves = [h(0x01), h(0x02)];
        assert!(build_proof(&leaves, 5).is_none());
    }

    #[test]
    fn chunks_root_matches_individual() {
        let data: &[&[u8]] = &[b"alpha", b"beta", b"gamma", b"delta"];
        let r1 = merkle_root_chunks(data);
        let leaves: Vec<[u8; 32]> = data
            .iter()
            .map(|c| {
                let mut buf = [0u8; 1 + 32];
                buf[0] = LEAF_PREFIX;
                let h = sha256::hash(c);
                buf[1..33].copy_from_slice(&h);
                sha256::hash(&buf)
            })
            .collect();
        let r2 = merkle_root(&leaves);
        assert_eq!(r1, r2);
    }

    #[test]
    fn chunks_empty_root() {
        assert_eq!(merkle_root_chunks(&[]), [0u8; 32]);
    }

    #[test]
    fn odd_leaf_count_uses_dup_up() {
        // 3 leaves: last level pairs (0,1) then (2,2 — duplicate).
        let r3 = merkle_root(&[h(0x01), h(0x02), h(0x03)]);
        let r4 = merkle_root(&[h(0x01), h(0x02), h(0x03), h(0x04)]);
        assert_ne!(r3, r4);
    }
}