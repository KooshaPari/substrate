//! Dijkstra's shortest-path algorithm on a non-negative-weighted graph.
//!
//! Operates on a simple representation: a weighted undirected graph where
//! edges are given as `(from, to, weight)` tuples. `n` is the number of
//! vertices (`0..n`). The algorithm uses a binary-heap priority queue
//! (provided by `std::collections::BinaryHeap`) with `(cost, node)` pairs.
//!
//! Reference: E. W. Dijkstra, "A note on two problems in connexion with
//! graphs", Numerische Mathematik, 1:269-271, 1959.
//!
//! Complexity: O((V + E) log V) using a min-heap; vertices may be disconnected
//! from the source (those stay at infinity).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Shortest-path distance record: `(distance, predecessor)`. If `pred` is
/// `None`, the node is the source (or unreachable, in which case `dist == INF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dist {
    /// Total cost from source; `u64::MAX` sentinel for unreachable.
    pub dist: u64,
    /// Previous vertex on the source->self path (or `None`).
    pub pred: Option<usize>,
}

/// Sentinel for unreachable nodes.
pub const INF: u64 = u64::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Edge {
    to: usize,
    weight: u64,
}

#[derive(Debug)]
pub struct Graph {
    /// Number of vertices.
    pub n: usize,
    /// Adjacency list per vertex.
    edges: Vec<Vec<Edge>>,
}

impl Graph {
    /// Build a graph with `n` vertices and no edges.
    pub fn new(n: usize) -> Self {
        Self {
            n,
            edges: vec![Vec::new(); n],
        }
    }

    /// Build a graph from `n` vertices and a slice of `(from, to, weight)`
    /// edges. Self-loops (`from == to`) are ignored.
    pub fn from_edges(n: usize, edges: &[(usize, usize, u64)]) -> Self {
        let mut g = Self::new(n);
        for &(u, v, w) in edges {
            if u == v || w == u64::MAX {
                continue;
            }
            g.edges[u].push(Edge { to: v, weight: w });
            g.edges[v].push(Edge { to: u, weight: w });
        }
        g
    }

    /// Add a directed edge `u -> v` with the given weight.
    pub fn add_directed(&mut self, u: usize, v: usize, w: u64) {
        if u < self.n && v < self.n && u != v && w != u64::MAX {
            self.edges[u].push(Edge { to: v, weight: w });
        }
    }

    /// Run Dijkstra from `src`. Returns a vector of `Dist` records indexed by
    /// vertex. Unreachable vertices get `dist = INF, pred = None`.
    pub fn dijkstra(&self, src: usize) -> Vec<Dist> {
        let mut dist: Vec<Dist> = (0..self.n)
            .map(|i| {
                if i == src {
                    Dist {
                        dist: 0,
                        pred: None,
                    }
                } else {
                    Dist {
                        dist: INF,
                        pred: None,
                    }
                }
            })
            .collect();

        let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
        heap.push(Reverse((0u64, src)));

        while let Some(Reverse((cost, u))) = heap.pop() {
            if cost > dist[u].dist {
                continue;
            }
            for &Edge { to, weight } in &self.edges[u] {
                let next = cost.saturating_add(weight);
                if next < dist[to].dist {
                    dist[to] = Dist {
                        dist: next,
                        pred: Some(u),
                    };
                    heap.push(Reverse((next, to)));
                }
            }
        }
        dist
    }

    /// Reconstruct the source->`target` path using predecessor links from a
    /// `dijkstra` result. Returns `None` if `target` is unreachable.
    pub fn path(dist: &[Dist], target: usize) -> Option<Vec<usize>> {
        if target >= dist.len() || dist[target].dist == INF {
            return None;
        }
        let mut rev = Vec::new();
        let mut cur = target;
        loop {
            rev.push(cur);
            match dist[cur].pred {
                Some(p) => cur = p,
                None => break,
            }
        }
        rev.reverse();
        Some(rev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_vertex() {
        let g = Graph::new(1);
        let d = g.dijkstra(0);
        assert_eq!(d[0].dist, 0);
        assert!(Graph::path(&d, 0).as_ref().unwrap() == &vec![0]);
    }

    #[test]
    fn disconnected_vertex() {
        // 0 - 1 - 2 ; 3 is alone.
        let g = Graph::from_edges(4, &[(0, 1, 1), (1, 2, 1)]);
        let d = g.dijkstra(0);
        assert_eq!(d[0].dist, 0);
        assert_eq!(d[1].dist, 1);
        assert_eq!(d[2].dist, 2);
        assert_eq!(d[3].dist, INF);
        assert!(Graph::path(&d, 3).is_none());
    }

    #[test]
    fn classic_dijkstra_paper_example() {
        // From Dijkstra (1959) example, transcribed:
        // 1-2 (7), 1-3 (9), 1-6 (14), 2-3 (10), 2-4 (15), 3-4 (11), 3-6 (2),
        // 4-5 (6), 5-6 (9). Source = 1, expected shortest 1->5 = 20 via 3-6-5.
        let edges = [
            (0usize, 1usize, 7u64),
            (0, 2, 9),
            (0, 5, 14),
            (1, 2, 10),
            (1, 3, 15),
            (2, 3, 11),
            (2, 5, 2),
            (3, 4, 6),
            (4, 5, 9),
        ];
        let g = Graph::from_edges(6, &edges);
        let d = g.dijkstra(0);
        // 0->4 via 2->5->4: 9 + 2 + 9 = 20.
        assert_eq!(d[4].dist, 20);
        // 0->3 via 2->3: 9 + 11 = 20.
        assert_eq!(d[3].dist, 20);
        // 0->5 via 2: 9 + 2 = 11.
        assert_eq!(d[5].dist, 11);
    }

    #[test]
    fn uniform_weight_grid_implicit() {
        // 4-chain: 0-1-2-3 each weight 5.
        let g = Graph::from_edges(4, &[(0, 1, 5), (1, 2, 5), (2, 3, 5)]);
        let d = g.dijkstra(0);
        assert_eq!(d[0].dist, 0);
        assert_eq!(d[1].dist, 5);
        assert_eq!(d[2].dist, 10);
        assert_eq!(d[3].dist, 15);
        let p = Graph::path(&d, 3).expect("reachable");
        assert_eq!(p, vec![0, 1, 2, 3]);
    }

    #[test]
    fn predecessor_chain_integrity() {
        let g = Graph::from_edges(
            6,
            &[
                (0, 1, 1),
                (0, 2, 1),
                (1, 3, 2),
                (2, 3, 2),
                (3, 4, 1),
                (4, 5, 1),
            ],
        );
        let d = g.dijkstra(0);
        for i in 0..g.n {
            if i == 0 {
                assert_eq!(d[i].pred, None);
            } else {
                assert!(d[i].pred.is_some());
            }
        }
        let p = Graph::path(&d, 5).expect("reachable");
        assert_eq!(p.first(), Some(&0));
        assert_eq!(p.last(), Some(&5));
    }

    #[test]
    fn revisit_path_sum_matches_dist() {
        // A small graph where one might double-count edges if mishandled.
        let g = Graph::from_edges(
            5,
            &[
                (0, 1, 4),
                (0, 2, 2),
                (1, 2, 1),
                (1, 3, 5),
                (2, 3, 8),
                (3, 4, 2),
            ],
        );
        let d = g.dijkstra(0);
        for (target, expected) in [(1, 3), (2, 2), (3, 8), (4, 10)] {
            assert_eq!(
                d[target].dist, expected,
                "vertex {target}: got {}, expected {expected}",
                d[target].dist
            );
        }
    }

    #[test]
    fn self_loop_ignored() {
        // A self-loop (u == v, weight > 0) must not poison the distance.
        let g = Graph::from_edges(2, &[(0, 0, 5), (0, 1, 7)]);
        let d = g.dijkstra(0);
        assert_eq!(d[0].dist, 0);
        assert_eq!(d[1].dist, 7);
    }

    #[test]
    fn directed_path_correctness() {
        // Directed graph: 0 -> 1 weight 1, 0 -> 2 weight 5, 1 -> 2 weight 1.
        let mut g = Graph::new(3);
        g.add_directed(0, 1, 1);
        g.add_directed(0, 2, 5);
        g.add_directed(1, 2, 1);
        let d = g.dijkstra(0);
        assert_eq!(d[2].dist, 2);
    }
}
