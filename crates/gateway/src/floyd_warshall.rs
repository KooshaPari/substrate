//! Floyd–Warshall all-pairs shortest paths.
//!
//! Computes the shortest-path distance between every pair of nodes in a
//! weighted directed graph. Supports negative edges (but not negative
//! cycles — caller should check the diagonal for negative values).
//!
//! `N` is a node-id type; we use `Vec<N>` plus an index map to support
//! any `Hash + Eq` node type.

use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Add;

/// Directed weighted edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge<N, C> {
    pub from: N,
    pub to: N,
    pub cost: C,
}

/// Adjacency list builder helper: `from -> Vec<(to, cost)>`.
pub fn build_adjacency<N: Hash + Eq + Clone, C: Copy>(
    edges: &[Edge<N, C>],
) -> HashMap<N, Vec<(N, C)>> {
    let mut adj: HashMap<N, Vec<(N, C)>> = HashMap::new();
    for e in edges {
        adj.entry(e.from.clone()).or_default().push((e.to.clone(), e.cost));
    }
    adj
}

/// Result of all-pairs shortest paths: distance[i][j] = shortest cost from
/// node `i` to node `j`. `None` means unreachable. The diagonal (i == j) is
/// always `Some(C::default())` for the zero-cost self-loop.
#[derive(Debug, Clone)]
pub struct AllPairs<C> {
    /// dist[i][j] = shortest path from i to j; None = unreachable
    pub dist: Vec<Vec<Option<C>>>,
    /// next[i][j] = Some(next node after i on the shortest path to j)
    pub next: Vec<Vec<Option<usize>>>,
    pub nodes: Vec<usize>, // index map; not currently used externally
}

impl<C: Copy + Default + Add<Output = C> + Ord> AllPairs<C> {
    pub fn distance(&self, from: usize, to: usize) -> Option<C> {
        self.dist.get(from).and_then(|row| row.get(to).copied().flatten())
    }
    /// Reconstruct shortest path from `from` to `to` (both node indices).
    pub fn path(&self, from: usize, to: usize) -> Option<Vec<usize>> {
        if self.dist.get(from)?.get(to)?.is_none() {
            return None;
        }
        if from == to {
            return Some(vec![from]);
        }
        let mut path = vec![from];
        let mut cur = from;
        while cur != to {
            let nxt = self.next[cur][to]?;
            path.push(nxt);
            cur = nxt;
        }
        Some(path)
    }
    /// Returns true if any node has a negative self-loop (negative cycle).
    pub fn has_negative_cycle(&self) -> bool
    where
        C: PartialOrd + Default,
    {
        let zero = C::default();
        for (i, row) in self.dist.iter().enumerate() {
            if let Some(Some(d)) = row.get(i) {
                if *d < zero {
                    return true;
                }
            }
        }
        false
    }
}

/// Compute all-pairs shortest paths.
///
/// `nodes` lists every node that may appear. Edges outside this list are
/// ignored. Returns the `AllPairs` structure with `dist` and `next` matrices.
pub fn floyd_warshall<N, C>(
    nodes: &[N],
    edges: &[Edge<N, C>],
) -> AllPairs<C>
where
    N: Hash + Eq + Clone,
    C: Copy + Default + Add<Output = C> + Ord,
{
    let n = nodes.len();
    let idx: HashMap<N, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.clone(), i))
        .collect();

    let mut dist: Vec<Vec<Option<C>>> = vec![vec![None; n]; n];
    let mut next: Vec<Vec<Option<usize>>> = vec![vec![None; n]; n];
    for i in 0..n {
        dist[i][i] = Some(C::default());
    }
    for e in edges {
        if let (Some(&i), Some(&j)) = (idx.get(&e.from), idx.get(&e.to)) {
            let entry = &mut dist[i][j];
            if entry.is_none() || Some(e.cost) < *entry {
                *entry = Some(e.cost);
                next[i][j] = Some(j);
            }
        }
    }

    for k in 0..n {
        for i in 0..n {
            if dist[i][k].is_none() {
                continue;
            }
            for j in 0..n {
                if dist[k][j].is_none() {
                    continue;
                }
                let via = dist[i][k].unwrap() + dist[k][j].unwrap();
                let direct = dist[i][j];
                let improved = match direct {
                    None => true,
                    Some(d) => via < d,
                };
                if improved {
                    dist[i][j] = Some(via);
                    // Path: i -> ... -> k -> ... -> j. The first hop after i
                    // is whatever `next[i][k]` was. If next[i][k] is None,
                    // then i == k (self-loop), so first hop is k=j? No:
                    // we recompute next for the i..j case using next[i][k].
                    let first_hop = next[i][k].unwrap_or(k);
                    next[i][j] = Some(first_hop);
                }
            }
        }
    }

    AllPairs {
        dist,
        next,
        nodes: (0..n).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_node_line() {
        let nodes = vec!["A", "B", "C"];
        let edges = vec![
            Edge { from: "A", to: "B", cost: 1u32 },
            Edge { from: "B", to: "C", cost: 2u32 },
        ];
        let ap = floyd_warshall(&nodes, &edges);
        assert_eq!(ap.distance(0, 2), Some(3));
        assert_eq!(ap.distance(0, 0), Some(0));
        assert_eq!(ap.distance(2, 0), None);
    }

    #[test]
    fn triangle_shortcut() {
        // A→B (1), B→C (1), A→C (5). A→B→C = 2 beats direct 5.
        let nodes = vec!["A", "B", "C"];
        let edges = vec![
            Edge { from: "A", to: "B", cost: 1u32 },
            Edge { from: "B", to: "C", cost: 1u32 },
            Edge { from: "A", to: "C", cost: 5u32 },
        ];
        let ap = floyd_warshall(&nodes, &edges);
        assert_eq!(ap.distance(0, 2), Some(2));
    }

    #[test]
    fn path_reconstruction() {
        let nodes = vec!["A", "B", "C", "D"];
        let edges = vec![
            Edge { from: "A", to: "B", cost: 1u32 },
            Edge { from: "B", to: "C", cost: 2u32 },
            Edge { from: "C", to: "D", cost: 3u32 },
        ];
        let ap = floyd_warshall(&nodes, &edges);
        let path = ap.path(0, 3).unwrap();
        assert_eq!(path, vec![0, 1, 2, 3]);
    }

    #[test]
    fn unreachable_pair() {
        let nodes = vec!["A", "B"];
        let edges = vec![Edge { from: "A", to: "B", cost: 5u32 }];
        let ap = floyd_warshall(&nodes, &edges);
        assert_eq!(ap.distance(0, 1), Some(5));
        assert_eq!(ap.distance(1, 0), None);
        assert!(ap.path(1, 0).is_none());
    }

    #[test]
    fn self_loop_zero_cost() {
        let nodes = vec![1, 2, 3];
        let edges: Vec<Edge<i32, u32>> = vec![];
        let ap = floyd_warshall(&nodes, &edges);
        assert_eq!(ap.distance(0, 0), Some(0));
        assert_eq!(ap.distance(1, 1), Some(0));
    }

    #[test]
    fn negative_edges_but_no_negative_cycle() {
        let nodes = vec!["A", "B", "C"];
        let edges = vec![
            Edge { from: "A", to: "B", cost: 4i32 },
            Edge { from: "B", to: "C", cost: -5i32 },
            Edge { from: "A", to: "C", cost: 1i32 },
        ];
        let ap = floyd_warshall(&nodes, &edges);
        // A→B→C = -1 < 1
        assert_eq!(ap.distance(0, 2), Some(-1));
        assert!(!ap.has_negative_cycle());
    }

    #[test]
    fn negative_cycle_detected() {
        let nodes = vec!["A", "B"];
        let edges = vec![
            Edge { from: "A", to: "B", cost: 1i32 },
            Edge { from: "B", to: "A", cost: -3i32 },
        ];
        let ap = floyd_warshall(&nodes, &edges);
        assert!(ap.has_negative_cycle());
    }

    #[test]
    fn build_adjacency_groups_edges() {
        let edges = vec![
            Edge { from: "X", to: "Y", cost: 1u32 },
            Edge { from: "X", to: "Z", cost: 2u32 },
            Edge { from: "Y", to: "Z", cost: 3u32 },
        ];
        let adj = build_adjacency(&edges);
        assert_eq!(adj.get("X").unwrap().len(), 2);
        assert_eq!(adj.get("Y").unwrap().len(), 1);
        assert!(adj.get("Z").is_none());
    }
}