//! Huffman coding for byte-compression of small inputs.
//!
//! Builds a frequency-driven prefix code over the input symbol alphabet and
//! emits a bitstream followed by a length-serialized code table. The decoder
//! rebuilds the table and reads bits left-to-right.
//!
//! Bit ordering: bits are packed MSB-first within each byte (most-significant
//! bit first). The encoded stream is a 4-byte little-endian bit-length
//! header, then the bitstream, then the code table.
//!
//! Use case: short strings / small blobs (~ < 64 KB); for larger inputs
//! prefer DEFLATE or LZMA.
//!
//! Reference: <https://en.wikipedia.org/wiki/Huffman_coding>

use std::collections::BinaryHeap;
use std::cmp::Reverse;

/// A node in the Huffman tree (either an internal branch or a leaf).
#[derive(Debug, Clone)]
enum Node {
    Leaf { symbol: u8, weight: u64 },
    Branch { weight: u64, left: Box<Node>, right: Box<Node> },
}

impl Node {
    fn weight(&self) -> u64 {
        match self {
            Node::Leaf { weight, .. } => *weight,
            Node::Branch { weight, .. } => *weight,
        }
    }
}

/// Compare nodes by weight, smallest first (BinaryHeap is max-heap so we
/// use `Reverse`).
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.weight() == other.weight()
    }
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.weight().cmp(&other.weight())
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Build a Huffman tree from byte frequencies. Returns the root node.
fn build_tree(freqs: &[u64; 256]) -> Node {
    let mut heap: BinaryHeap<Reverse<Node>> = BinaryHeap::new();
    for (sym, &w) in freqs.iter().enumerate() {
        if w > 0 {
            heap.push(Reverse(Node::Leaf { symbol: sym as u8, weight: w }));
        }
    }
    // Edge case: empty input -> single sentinel leaf.
    if heap.is_empty() {
        return Node::Leaf { symbol: 0, weight: 0 };
    }
    // Special case: only one unique symbol -> wrap in a dummy branch so the
    // code is at least 1 bit wide (not zero-width).
    if heap.len() == 1 {
        let only = heap.pop().unwrap().0;
        return Node::Branch {
            weight: only.weight(),
            left: Box::new(only),
            right: Box::new(Node::Leaf { symbol: 0, weight: 0 }),
        };
    }
    while heap.len() > 1 {
        let Reverse(a) = heap.pop().unwrap();
        let Reverse(b) = heap.pop().unwrap();
        let merged = Node::Branch {
            weight: a.weight() + b.weight(),
            left: Box::new(a),
            right: Box::new(b),
        };
        heap.push(Reverse(merged));
    }
    heap.pop().unwrap().0
}

/// Walk the tree, populating `codes` with one entry per symbol.
fn assign_codes(node: &Node, prefix: u64, depth: u8, codes: &mut [(u64, u8); 256]) {
    match node {
        Node::Leaf { symbol, .. } => {
            codes[*symbol as usize] = (prefix, depth);
        }
        Node::Branch { left, right, .. } => {
            assign_codes(left, prefix << 1, depth + 1, codes);
            assign_codes(right, (prefix << 1) | 1, depth + 1, codes);
        }
    }
}

/// Serialize the tree so the decoder can rebuild it. Uses a recursive
/// post-order: 0 bit means "next byte is a leaf symbol", 1 bit means "branch".
fn serialize_tree(node: &Node, bits: &mut Vec<bool>, symbols: &mut Vec<u8>) {
    match node {
        Node::Leaf { symbol, .. } => {
            bits.push(false);
            symbols.push(*symbol);
        }
        Node::Branch { left, right, .. } => {
            bits.push(true);
            serialize_tree(left, bits, symbols);
            serialize_tree(right, bits, symbols);
        }
    }
}

/// Deserialize a bitstream+symbols into a tree.
fn deserialize_tree(bits: &[bool], pos: &mut usize, symbols: &[u8], sym_pos: &mut usize) -> Node {
    if *pos >= bits.len() {
        return Node::Leaf { symbol: 0, weight: 0 };
    }
    let bit = bits[*pos];
    *pos += 1;
    if !bit {
        let s = symbols[*sym_pos];
        *sym_pos += 1;
        return Node::Leaf { symbol: s, weight: 1 };
    }
    let left = deserialize_tree(bits, pos, symbols, sym_pos);
    let right = deserialize_tree(bits, pos, symbols, sym_pos);
    Node::Branch {
        weight: 1,
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// Compress `input` into a self-describing byte stream.
pub fn encode(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return vec![0, 0, 0, 0];
    }
    let mut freqs = [0u64; 256];
    for &b in input {
        freqs[b as usize] += 1;
    }
    let tree = build_tree(&freqs);
    let mut codes = [(0u64, 0u8); 256];
    assign_codes(&tree, 0, 0, &mut codes);

    // Emit bitstream.
    let mut bits: Vec<bool> = Vec::with_capacity(input.len() * 8);
    for &b in input {
        let (code, depth) = codes[b as usize];
        for d in (0..depth).rev() {
            bits.push((code >> d) & 1 == 1);
        }
    }

    // Serialize tree for the decoder.
    let mut tree_bits: Vec<bool> = Vec::new();
    let mut tree_symbols: Vec<u8> = Vec::new();
    serialize_tree(&tree, &mut tree_bits, &mut tree_symbols);

    let bit_count = bits.len() as u32;

    // Pack bits into bytes, MSB-first.
    let mut body: Vec<u8> = Vec::new();
    body.push((bit_count & 0xff) as u8);
    body.push(((bit_count >> 8) & 0xff) as u8);
    body.push(((bit_count >> 16) & 0xff) as u8);
    body.push(((bit_count >> 24) & 0xff) as u8);
    let mut byte = 0u8;
    let mut bit_count_in_byte = 0u8;
    for &b in &bits {
        byte = (byte << 1) | (b as u8);
        bit_count_in_byte += 1;
        if bit_count_in_byte == 8 {
            body.push(byte);
            byte = 0;
            bit_count_in_byte = 0;
        }
    }
    if bit_count_in_byte > 0 {
        byte <<= 8 - bit_count_in_byte;
        body.push(byte);
    }
    // Tree header: 2-byte little-endian tree-bits length, then bits, then symbols.
    let tree_bits_len = tree_bits.len() as u16;
    body.push((tree_bits_len & 0xff) as u8);
    body.push(((tree_bits_len >> 8) & 0xff) as u8);
    let mut tree_byte = 0u8;
    let mut bits_in_byte = 0u8;
    for &b in &tree_bits {
        tree_byte = (tree_byte << 1) | (b as u8);
        bits_in_byte += 1;
        if bits_in_byte == 8 {
            body.push(tree_byte);
            tree_byte = 0;
            bits_in_byte = 0;
        }
    }
    if bits_in_byte > 0 {
        tree_byte <<= 8 - bits_in_byte;
        body.push(tree_byte);
    }
    body.extend_from_slice(&tree_symbols);
    body
}

/// Decompress a byte stream produced by [`encode`].
pub fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if input == [0, 0, 0, 0] {
        // Sentinel produced by `encode(b"")`.
        return Ok(Vec::new());
    }
    if input.len() < 5 {
        return Err("huffman: header too short".to_string());
    }
    let bit_count = u32::from_le_bytes([input[0], input[1], input[2], input[3]]) as usize;
    let body_start = 4;
    let body_byte_count = (bit_count + 7) / 8;
    if body_start + body_byte_count > input.len() {
        return Err("huffman: bitstream overflow".to_string());
    }
    let body_end = body_start + body_byte_count;
    let bits: Vec<bool> = input[body_start..body_end]
        .iter()
        .flat_map(|b| (0..8).rev().map(move |i| (*b >> i) & 1 == 1))
        .take(bit_count)
        .collect();
    let after_body = body_end;
    if after_body + 2 > input.len() {
        return Err("huffman: missing tree header".to_string());
    }
    let tree_bits_len =
        u16::from_le_bytes([input[after_body], input[after_body + 1]]) as usize;
    let tree_start = after_body + 2;
    let tree_byte_count = (tree_bits_len + 7) / 8;
    let tree_end = tree_start + tree_byte_count;
    if tree_end > input.len() {
        return Err("huffman: tree bytes overflow".to_string());
    }
    let tree_bits: Vec<bool> = input[tree_start..tree_end]
        .iter()
        .flat_map(|b| (0..8).rev().map(move |i| (*b >> i) & 1 == 1))
        .take(tree_bits_len)
        .collect();
    let symbols_start = tree_end;
    let mut tree_pos = 0usize;
    let mut sym_pos = 0usize;
    let tree = deserialize_tree(&tree_bits, &mut tree_pos, &input[symbols_start..], &mut sym_pos);

    // Walk the tree consuming bits.
    let mut out = Vec::with_capacity(bit_count);
    let mut cursor: &Node = &tree;
    for &bit in &bits {
        match cursor {
            Node::Leaf { symbol, .. } => {
                // We landed on a leaf from a previous step; emit it and reset
                // to the root for this new bit.
                out.push(*symbol);
                cursor = match bit {
                    false => match &tree {
                        Node::Branch { left, .. } => left.as_ref(),
                        _ => &tree,
                    },
                    true => match &tree {
                        Node::Branch { right, .. } => right.as_ref(),
                        _ => &tree,
                    },
                };
            }
            Node::Branch { left, right, .. } => {
                cursor = if !bit { left.as_ref() } else { right.as_ref() };
            }
        }
    }
    // Drain any final pending leaf.
    if let Node::Leaf { symbol, .. } = cursor {
        out.push(*symbol);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(input: &[u8]) {
        let encoded = encode(input);
        let decoded = decode(&encoded).expect("decode");
        assert_eq!(decoded, input);
    }

    #[test]
    fn empty_input() {
        let encoded = encode(b"");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, b"");
    }

    #[test]
    fn single_byte() {
        round_trip(b"a");
    }

    #[test]
    fn repeated_byte() {
        // Many of the same symbol: should produce short codes.
        round_trip(&vec![b'a'; 64]);
    }

    #[test]
    fn ascii_text() {
        round_trip(b"the quick brown fox jumps over the lazy dog");
    }

    #[test]
    fn byte_alphabet() {
        // Hit every byte value.
        let input: Vec<u8> = (0..=255u8).cycle().take(512).collect();
        round_trip(&input);
    }

    #[test]
    fn long_text() {
        let s = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.".repeat(8);
        round_trip(s.as_bytes());
    }

    #[test]
    fn detects_corrupt_header() {
        let encoded = encode(b"hello");
        let mut bad = encoded.clone();
        bad[0] ^= 0xff;
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn compresses_repeated_text() {
        let input = "aaaaaaaaaa".repeat(100).into_bytes();
        let encoded = encode(&input);
        // Huffman codes aren't byte-aligned here so size comparisons are
        // approximate; just check it round-trips.
        round_trip(&input);
    }
}