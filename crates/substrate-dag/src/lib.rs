#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! DAG workflow orchestration via [`WorkflowPort`] and petgraph.

use std::collections::{HashMap, HashSet};

use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use substrate_core::error::{Result, SubstrateError};
use substrate_core::workflow_port::{Workflow, WorkflowPort};

/// [`WorkflowPort`] backed by petgraph directed graphs.
#[derive(Debug, Default, Clone, Copy)]
pub struct DagWorkflow;

impl DagWorkflow {
    /// Create a new workflow engine.
    pub fn new() -> Self {
        Self
    }

    fn build_graph(
        workflow: &Workflow,
    ) -> Result<(DiGraph<String, ()>, HashMap<String, NodeIndex>)> {
        let mut graph = DiGraph::new();
        let mut index_of = HashMap::new();

        for node in &workflow.nodes {
            if index_of.contains_key(&node.id) {
                return Err(SubstrateError::Other(format!(
                    "duplicate node id: {}",
                    node.id
                )));
            }
            let idx = graph.add_node(node.id.clone());
            index_of.insert(node.id.clone(), idx);
        }

        for edge in &workflow.edges {
            let from = index_of.get(&edge.from).ok_or_else(|| {
                SubstrateError::Other(format!("unknown edge source: {}", edge.from))
            })?;
            let to = index_of.get(&edge.to).ok_or_else(|| {
                SubstrateError::Other(format!("unknown edge target: {}", edge.to))
            })?;
            graph.add_edge(*from, *to, ());
        }

        Ok((graph, index_of))
    }
}

impl WorkflowPort for DagWorkflow {
    fn validate_acyclic(&self, workflow: &Workflow) -> Result<()> {
        let (graph, _) = Self::build_graph(workflow)?;
        if is_cyclic_directed(&graph) {
            return Err(SubstrateError::CycleDetected(
                "workflow contains a cycle".into(),
            ));
        }
        Ok(())
    }

    fn topological_order(&self, workflow: &Workflow) -> Result<Vec<String>> {
        self.validate_acyclic(workflow)?;
        let (graph, index_of) = Self::build_graph(workflow)?;

        let mut in_degree: HashMap<NodeIndex, usize> =
            graph.node_indices().map(|n| (n, 0)).collect();
        for edge in graph.edge_indices() {
            let (_, target) = graph.edge_endpoints(edge).unwrap();
            *in_degree.get_mut(&target).unwrap() += 1;
        }

        let mut ready: Vec<NodeIndex> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&n, _)| n)
            .collect();
        ready.sort_by_key(|n| graph[*n].clone());

        let mut order = Vec::with_capacity(graph.node_count());
        while let Some(n) = ready.first().cloned() {
            ready.remove(0);
            order.push(graph[n].clone());
            for child in graph.neighbors_directed(n, Direction::Outgoing) {
                let deg = in_degree.get_mut(&child).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    ready.push(child);
                    ready.sort_by_key(|i| graph[*i].clone());
                }
            }
        }

        if order.len() != graph.node_count() {
            return Err(SubstrateError::CycleDetected(
                "topological sort incomplete — cycle present".into(),
            ));
        }
        let _ = index_of;
        Ok(order)
    }

    fn ready_set(&self, workflow: &Workflow, completed: &[String]) -> Result<Vec<String>> {
        self.validate_acyclic(workflow)?;
        let (graph, index_of) = Self::build_graph(workflow)?;
        let done: HashSet<&str> = completed.iter().map(String::as_str).collect();

        let mut ready = Vec::new();
        for node in &workflow.nodes {
            if done.contains(node.id.as_str()) {
                continue;
            }
            let idx = index_of[&node.id];
            let mut preds = graph.neighbors_directed(idx, Direction::Incoming);
            if preds.all(|p| done.contains(graph[p].as_str())) {
                ready.push(node.id.clone());
            }
        }
        ready.sort();
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use substrate_core::workflow_port::{WorkflowEdge, WorkflowNode};

    fn wf(nodes: &[&str], edges: &[(&str, &str)]) -> Workflow {
        Workflow {
            nodes: nodes
                .iter()
                .map(|id| WorkflowNode { id: (*id).into() })
                .collect(),
            edges: edges
                .iter()
                .map(|(from, to)| WorkflowEdge {
                    from: (*from).into(),
                    to: (*to).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn linear_chain_topological_order() {
        let dag = DagWorkflow::new();
        let w = wf(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let order = dag.topological_order(&w).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn diamond_ready_set_after_partial() {
        let dag = DagWorkflow::new();
        let w = wf(
            &["a", "b", "c", "d"],
            &[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")],
        );
        let ready0 = dag.ready_set(&w, &[]).unwrap();
        assert_eq!(ready0, vec!["a"]);

        let ready1 = dag.ready_set(&w, &["a".into()]).unwrap();
        assert_eq!(ready1, vec!["b", "c"]);

        let ready2 = dag
            .ready_set(&w, &["a".into(), "b".into(), "c".into()])
            .unwrap();
        assert_eq!(ready2, vec!["d"]);
    }

    #[test]
    fn cycle_rejected() {
        let dag = DagWorkflow::new();
        let w = wf(&["a", "b", "c"], &[("a", "b"), ("b", "c"), ("c", "a")]);
        let err = dag.validate_acyclic(&w).unwrap_err();
        assert!(matches!(err, SubstrateError::CycleDetected(_)));
    }
}
