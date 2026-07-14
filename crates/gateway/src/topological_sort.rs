//! Topological sort — Kahn's algorithm for Directed Acyclic Graphs (DAGs).
//!
//! Given a set of nodes and directed edges between them, returns a linear
//! ordering such that for every edge `u -> v`, `u` appears before `v` in
//! the order. If the graph contains a cycle, returns `None`.
//!
//! Reference: A. B. Kahn, "Topological sorting of large networks",
//! Communications of the ACM, 1962.
//!
//! `Node` is a generic identifier type. The caller supplies the node set
//! and the adjacency list `edges` mapping each node to its outgoing
//! successors. Both are typically built from the caller's data without
//! copying.
//!
//! The runtime is O(V + E) and the algorithm is iterative, so it handles
//! very large DAGs without stack growth.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Topologically sort the nodes of a directed acyclic graph.
///
/// `nodes` is the full set of node identifiers; `edges` maps each node
/// to its outgoing successors. The order returned uses one valid
/// topological ordering (not necessarily unique). Returns `None` if a
/// cycle is detected: in that case no linear ordering exists.
///
/// Duplicate entries in `edges` for the same target are tolerated.
/// Nodes that never appear as keys in `edges` are still included in the
/// output (with no predecessors themselves they sort earliest).
pub fn topo_sort<Node>(nodes: &[Node], edges: &HashMap<Node, Vec<Node>>) -> Option<Vec<Node>>
where
    Node: Eq + Hash + Clone,
{
    // Build in-degree map and adjacency list. We treat missing keys as
    // "no outgoing edges" so callers can pass partial adjacency maps.
    let mut in_degree: HashMap<Node, usize> = HashMap::with_capacity(nodes.len());
    for n in nodes {
        in_degree.entry(n.clone()).or_insert(0);
    }
    // Copy adjacency into a HashMap<Vec<Node>> to dedupe-targets so a
    // duplicate edge doesn't inflate in-degree.
    let mut adj: HashMap<Node, Vec<Node>> = HashMap::with_capacity(nodes.len());
    for (u, vs) in edges {
        let mut seen = std::collections::HashSet::new();
        let mut uniq: Vec<Node> = Vec::with_capacity(vs.len());
        for v in vs {
            if seen.insert(v.clone()) {
                uniq.push(v.clone());
            }
        }
        adj.insert(u.clone(), uniq);
    }
    for vs in adj.values() {
        for v in vs {
            *in_degree.entry(v.clone()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<Node> = VecDeque::new();
    for (n, d) in &in_degree {
        if *d == 0 {
            queue.push_back(n.clone());
        }
    }

    let mut order: Vec<Node> = Vec::with_capacity(nodes.len());
    while let Some(u) = queue.pop_front() {
        order.push(u.clone());
        if let Some(succs) = adj.get(&u) {
            for v in succs {
                if let Some(d) = in_degree.get_mut(v) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(v.clone());
                    }
                }
            }
        }
    }

    if order.len() == nodes.len() {
        Some(order)
    } else {
        None
    }
}

/// Detect whether a directed graph contains a cycle.
///
/// Implemented via the same machinery as `topo_sort` but only the
/// boolean result is returned. O(V + E).
pub fn has_cycle<Node>(nodes: &[Node], edges: &HashMap<Node, Vec<Node>>) -> bool
where
    Node: Eq + Hash + Clone,
{
    topo_sort(nodes, edges).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(
        g: &[(&'static str, &'static [&'static str])],
    ) -> (Vec<&'static str>, HashMap<&'static str, Vec<&'static str>>) {
        let nodes: Vec<&'static str> = g.iter().map(|(n, _)| *n).collect();
        let mut edges: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        for (n, vs) in g {
            edges.insert(*n, vs.to_vec());
        }
        (nodes, edges)
    }

    #[test]
    fn empty_graph() {
        let n: &[&'static str] = &[];
        let e: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        let r: Vec<&'static str> = topo_sort(n, &e).unwrap();
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn single_node_no_edges() {
        let (n, e) = make(&[("a", &[])]);
        let r = topo_sort(&n, &e).unwrap();
        assert_eq!(r, vec!["a"]);
    }

    #[test]
    fn linear_chain() {
        // a -> b -> c -> d
        let (n, e) = make(&[("a", &["b"]), ("b", &["c"]), ("c", &["d"]), ("d", &[])]);
        let r = topo_sort(&n, &e).unwrap();
        assert_eq!(r, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn diamond_dag() {
        // a -> {b,c}; b -> d; c -> d
        let (n, e) = make(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"]), ("d", &[])]);
        let r = topo_sort(&n, &e).unwrap();
        assert_eq!(r[0], "a");
        assert_eq!(r[3], "d");
        // b before d and c before d
        let pos = |x: &str| r.iter().position(|&y| y == x).unwrap();
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
    }

    #[test]
    fn cycle_detected() {
        // a -> b -> c -> a
        let (n, e) = make(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"])]);
        assert!(topo_sort(&n, &e).is_none());
        assert!(has_cycle(&n, &e));
    }

    #[test]
    fn self_loop_detected() {
        let (n, e) = make(&[("a", &["a"])]);
        assert!(topo_sort(&n, &e).is_none());
        assert!(has_cycle(&n, &e));
    }

    #[test]
    fn duplicate_edges_ignored() {
        // a -> b twice; should still produce a valid order.
        let (n, e) = make(&[("a", &["b", "b"]), ("b", &[])]);
        let r = topo_sort(&n, &e).unwrap();
        assert_eq!(r, vec!["a", "b"]);
    }

    #[test]
    fn disconnected_components() {
        // Two chains: a -> b, and x -> y.
        let (n, e) = make(&[("a", &["b"]), ("b", &[]), ("x", &["y"]), ("y", &[])]);
        let r = topo_sort(&n, &e).unwrap();
        let pos = |x: &str| r.iter().position(|&y| y == x).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("x") < pos("y"));
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn missing_adjacency_keys_are_terminals() {
        // 'a' has no entry in edges but is a node.
        let mut nodes: Vec<&str> = vec!["a"];
        let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
        edges.insert("b", vec!["c"]);
        nodes.push("b");
        nodes.push("c");
        let r = topo_sort(&nodes, &edges).unwrap();
        assert_eq!(r.len(), 3);
        let pos = |x: &str| r.iter().position(|&y| y == x).unwrap();
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn build_order_classic() {
        // Compile-time dep graph: tools -> compiler -> linker -> exe;
        // tools -> linker.
        let (n, e) = make(&[
            ("tools", &["compiler", "linker"]),
            ("compiler", &["linker"]),
            ("linker", &["exe"]),
            ("exe", &[]),
        ]);
        let r = topo_sort(&n, &e).unwrap();
        let pos = |x: &str| r.iter().position(|&y| y == x).unwrap();
        assert!(pos("tools") < pos("compiler"));
        assert!(pos("compiler") < pos("linker"));
        assert!(pos("linker") < pos("exe"));
    }
}
