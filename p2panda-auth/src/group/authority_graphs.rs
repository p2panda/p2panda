// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::prelude::DiGraphMap;

use crate::graph::{has_path, is_concurrent};
use crate::traits::{IdentityHandle, OperationId};

/// Removal graph edge (remover, removed, operation id).
type Removal<ID, OP> = (ID, ID, OP);

/// Delegation graph edge (delegator, delegate, operation id).
type Delegation<ID, OP> = (ID, ID, OP);

/// A graph node is the tuple of (actor id, operation id). For every operation added to the graph
/// a new node is created for each involved actor. This is so that the authority graph can reason
/// about concurrent operations when adding edges.
type Node<ID, OP> = (ID, OP);

/// An authority graph mapping removals and delegations between peers. It is concurrency aware and
/// and used for detecting mutual-remove cycles. Uses [Tarjan's
/// algorithm](https://en.wikipedia.org/wiki/Tarjan%27s_strongly_connected_components_algorithm)
/// for detecting strongly connected components under the hood.
///
/// Being concurrency aware is required in order to only add removal edges to the graph when the
/// removal occurred concurrently to a removal or delegation, and to only add delegation edges
/// when they were _not_ concurrent.
///
/// ## Example mutual remove cycle
///
/// Operation Graph
///
/// ```ignore
///         0
///       / | \
///      1  2  3
///            |
///            4
///   
/// 0: Initial group state {Alice, Bob, Claire}
/// 1: Alice removes Bob
/// 2: Bob removes Claire
/// 3: Claire adds Dave
/// 4: Dave removes Alice
///
/// ```
///
/// Authority Graph
///
/// ```ignore
///      ______________
///     |              |
///     ▼              |
///   Alice            |
///     │ (1: remove)  |
///     ▼              |
///    Bob             |
///     │ (2: remove)  |
///     ▼              |
///  Claire            |
///     │ (3: add)     |
///     ▼              |
///    Dave            |
///     │ (4: remove)  |
///     |______________|
///
/// ```
///
#[derive(Debug)]
pub struct AuthorityGraphs<ID, OP>
where
    ID: IdentityHandle + Ord,
    OP: OperationId + Ord,
{
    deps_graph: DiGraphMap<OP, ()>,
    removals: HashMap<ID, Vec<Removal<ID, OP>>>,
    delegations: HashMap<ID, Vec<Delegation<ID, OP>>>,
    graphs: HashMap<ID, DiGraph<Node<ID, OP>, ()>>,
    cycles: HashMap<ID, HashSet<OP>>,
}

impl<ID, OP> AuthorityGraphs<ID, OP>
where
    ID: IdentityHandle + Ord,
    OP: OperationId + Ord,
{
    pub fn new(deps_graph: DiGraphMap<OP, ()>) -> Self {
        Self {
            deps_graph,
            removals: HashMap::default(),
            delegations: HashMap::default(),
            graphs: HashMap::default(),
            cycles: HashMap::default(),
        }
    }

    /// Register a new removal.
    pub fn add_removal(&mut self, group_id: ID, remover: ID, removed: ID, op: OP) {
        if remover == removed {
            return;
        }
        self.removals
            .entry(group_id)
            .or_default()
            .push((remover, removed, op));

        // Smash the cache.
        self.graphs.remove(&group_id);
        self.cycles.remove(&group_id);
    }

    /// Register a new delegation.
    pub fn add_delegation(&mut self, group_id: ID, delegator: ID, delegate: ID, op: OP) {
        self.delegations
            .entry(group_id)
            .or_default()
            .push((delegator, delegate, op));

        // Smash the cache.
        self.graphs.remove(&group_id);
        self.cycles.remove(&group_id);
    }

    /// Returns true if the target operation is part of a mutual remove cycle.
    pub fn is_cycle(&mut self, group_id: &ID, target_op: &OP) -> bool {
        // If the graph is none (the cache was busted) then rebuild graph and cycle state.
        if self.graphs.get(group_id).is_none() {
            self.build_graph(&group_id);
            self.compute_cycles(&group_id);
        }

        if let Some(set) = self.cycles.get(group_id) {
            return set.contains(target_op);
        }

        false
    }

    fn build_graph(&mut self, group_id: &ID) {
        let removals = self.removals.get(group_id);
        let delegations = self.delegations.get(group_id);

        if removals.is_none() && delegations.is_none() {
            return;
        }

        let mut graph = DiGraph::new();
        let mut nodes = HashMap::<Node<ID, OP>, NodeIndex>::new();

        // Add nodes for removals.
        if let Some(removals) = removals {
            for (remover, removed, op) in removals {
                let from_idx = Self::ensure_node(&mut graph, &mut nodes, (*remover, *op));
                let to_idx = Self::ensure_node(&mut graph, &mut nodes, (*removed, *op));

                Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
            }
        }

        // Add nodes for delegations.
        if let Some(delegations) = delegations {
            for (delegator, delegate, op) in delegations {
                let from_idx = Self::ensure_node(&mut graph, &mut nodes, (*delegator, *op));
                let to_idx = Self::ensure_node(&mut graph, &mut nodes, (*delegate, *op));

                Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
            }
        }

        // Add edges for removals.
        if let Some(removals) = removals {
            for (remover, removed, op) in removals {
                for (remover_inner, removed_inner, op_inner) in removals {
                    // If the removals are not concurrent don't add any edges.
                    if !is_concurrent(&self.deps_graph, *op, *op_inner) {
                        continue;
                    }

                    // One edge is added for every concurrent removal between (actor, operation
                    // id) nodes.
                    if removed == remover_inner {
                        let from_idx = nodes[&(*removed, *op)];
                        let to_idx = nodes[&(*remover_inner, *op_inner)];
                        Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
                    }

                    if removed_inner == remover {
                        let from_idx = nodes[&(*removed_inner, *op_inner)];
                        let to_idx = nodes[&(*remover, *op)];
                        Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
                    }
                }
            }
        }

        if let (Some(removals), Some(delegations)) = (removals, delegations) {
            // Add removal -> delegation edges and delegation -> removal edges
            for (delegator, delegate, op) in delegations {
                for (remover, removed, op_inner) in removals {
                    // Check if the delegation and removal are concurrent.
                    let is_connected = has_path(&self.deps_graph, *op, *op_inner);

                    // Only add the removal -> delegation edge if the removal occurred concurrently.
                    if removed == delegator && !is_connected {
                        let from_idx = nodes[&(*removed, *op_inner)];
                        let to_idx = nodes[&(*delegator, *op)];
                        Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
                    }

                    // Only add the delegation -> removal edge if the removal is a successor of
                    // the delegation.
                    if delegate == remover && is_connected {
                        let from_idx = nodes[&(*delegate, *op)];
                        let to_idx = nodes[&(*remover, *op_inner)];
                        Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
                    }
                }

                // Add delegation -> delegation edges.
                for (delegator_inner, _, op_inner) in delegations {
                    let is_connected = has_path(&self.deps_graph, *op, *op_inner);
                    if delegate == delegator_inner && is_connected {
                        let from_idx = nodes[&(*delegate, *op)];
                        let to_idx = nodes[&(*delegator_inner, *op_inner)];
                        Self::add_edge_if_missing(&mut graph, from_idx, to_idx);
                    }
                }
            }
        }

        self.graphs.insert(group_id.clone(), graph);
    }

    fn ensure_node(
        graph: &mut DiGraph<Node<ID, OP>, ()>,
        nodes: &mut HashMap<Node<ID, OP>, NodeIndex>,
        key: Node<ID, OP>,
    ) -> NodeIndex {
        *nodes
            .entry(key.clone())
            .or_insert_with(|| graph.add_node(key))
    }

    fn add_edge_if_missing(graph: &mut DiGraph<Node<ID, OP>, ()>, from: NodeIndex, to: NodeIndex) {
        if graph.find_edge(from, to).is_none() {
            graph.add_edge(from, to, ());
        }
    }

    /// Compute cycles for a group.
    fn compute_cycles(&mut self, group_id: &ID) {
        let graph = match self.graphs.get(group_id) {
            Some(g) => g,
            None => {
                self.cycles.insert(group_id.clone(), HashSet::new());
                return;
            }
        };

        // Run Tarjan's algorithm to detect cycles (strongly connected components).
        let sccs = petgraph::algo::tarjan_scc(graph);
        let mut ops_in_cycles: HashSet<OP> = HashSet::new();
        for scc in sccs {
            if scc.len() < 2 {
                continue;
            }
            for &node_idx in &scc {
                let &(_id, op) = &graph[node_idx];
                ops_in_cycles.insert(op);
            }
        }

        self.cycles.insert(group_id.clone(), ops_in_cycles);
    }
}

#[cfg(test)]
mod tests {
    use petgraph::graphmap::DiGraphMap;

    use super::AuthorityGraphs;

    #[test]
    fn graph_builds() {
        let a = 'A';
        let b = 'B';
        let group = 'G';

        let op0: u32 = 0;
        let op1: u32 = 1;

        // No dependencies between nodes, all are concurrent.
        let dep_graph = DiGraphMap::<u32, ()>::new();

        let mut authority = AuthorityGraphs::new(dep_graph);

        // A removes B
        // B removes A
        authority.add_removal(group, a, b, op0);
        authority.add_removal(group, b, a, op1);

        // Manually build the graph.
        authority.build_graph(&group);

        let graph = authority.graphs.get(&group).unwrap();

        let a_op0 = (a, op0);
        let b_op0 = (b, op0);
        let b_op1 = (b, op1);
        let a_op1 = (a, op1);

        let a_op0_idx = graph.node_indices().find(|&i| graph[i] == a_op0).unwrap();
        let b_op0_idx = graph.node_indices().find(|&i| graph[i] == b_op0).unwrap();
        let b_op1_idx = graph.node_indices().find(|&i| graph[i] == b_op1).unwrap();
        let a_op1_idx = graph.node_indices().find(|&i| graph[i] == a_op1).unwrap();

        assert_eq!(graph.edge_count(), 4);

        assert!(graph.find_edge(a_op0_idx, b_op0_idx).is_some());
        assert!(graph.find_edge(b_op0_idx, b_op1_idx).is_some());
        assert!(graph.find_edge(b_op1_idx, a_op1_idx).is_some());
        assert!(graph.find_edge(a_op1_idx, a_op0_idx).is_some());
    }

    #[test]
    fn removal_cycle() {
        let a = 'A';
        let b = 'B';
        let c = 'C';
        let group = 'G';

        let op0: u32 = 0;
        let op1: u32 = 1;
        let op2: u32 = 2;
        let op3: u32 = 3;
        let op4: u32 = 4;
        let op5: u32 = 5;

        // Operation dependency graph:
        //
        //    0
        //  / | \
        // 1  2  3
        //  \ | /
        //    4
        //    |
        //    5
        //
        let mut dep_graph = DiGraphMap::<u32, ()>::new();
        dep_graph.add_node(op0);
        dep_graph.add_node(op1);
        dep_graph.add_node(op2);
        dep_graph.add_node(op3);
        dep_graph.add_node(op4);
        dep_graph.add_node(op5);

        dep_graph.add_edge(op0, op1, ());
        dep_graph.add_edge(op0, op2, ());
        dep_graph.add_edge(op0, op3, ());
        dep_graph.add_edge(op1, op4, ());
        dep_graph.add_edge(op2, op4, ());
        dep_graph.add_edge(op3, op4, ());
        dep_graph.add_edge(op4, op5, ());

        // Cycle:
        //
        // 1: A remove B
        // 2: B remove C
        // 3: C remove A
        //
        let mut authority = AuthorityGraphs::new(dep_graph);

        authority.add_removal(group, a, b, op1);
        authority.add_removal(group, b, c, op2);
        authority.add_removal(group, c, a, op3);

        // This removal is not part of a cycle.
        authority.add_removal(group, c, a, op5);

        assert!(!authority.is_cycle(&group, &op0));
        assert!(authority.is_cycle(&group, &op1));
        assert!(authority.is_cycle(&group, &op2));
        assert!(authority.is_cycle(&group, &op3));
        assert!(!authority.is_cycle(&group, &op4));
        assert!(!authority.is_cycle(&group, &op5));
    }

    #[test]
    fn remove_delegate_cycle() {
        let a = 'A';
        let b = 'B';
        let c = 'C';
        let group = 'G';

        let op0: u32 = 0;
        let op1: u32 = 1;
        let op2: u32 = 2;
        let op3: u32 = 3;

        // Operation dependency graph:
        //
        // 0
        // | \
        // 1  3
        // |
        // 2
        //
        let mut dep_graph = DiGraphMap::<u32, ()>::new();
        dep_graph.add_node(op0);
        dep_graph.add_node(op1);
        dep_graph.add_node(op2);

        dep_graph.add_edge(op0, op1, ());
        dep_graph.add_edge(op1, op2, ());
        dep_graph.add_edge(op0, op3, ());

        // Cycle:
        //
        // 1: B delegate C
        // 2: C remove A
        // 3: A remove B
        //
        let mut authority = AuthorityGraphs::new(dep_graph);
        authority.add_delegation(group, b, c, op1);
        authority.add_removal(group, c, a, op2);
        authority.add_removal(group, a, b, op3);

        assert!(authority.is_cycle(&group, &op1));
        assert!(authority.is_cycle(&group, &op2));
        assert!(authority.is_cycle(&group, &op3));
    }

    #[test]
    fn multi_delegate_chains() {
        let a = 'A';
        let b = 'B';
        let c = 'C';
        let d = 'D';
        let e = 'E';
        let group = 'G';

        let op0: u32 = 0;
        let op1: u32 = 1;
        let op2: u32 = 2;
        let op3: u32 = 3;
        let op4: u32 = 4;

        // Operation dependency graph:
        //
        // 0  1  2
        //       |
        //       3
        //       |
        //       4
        //
        let mut dep_graph = DiGraphMap::<u32, ()>::new();
        dep_graph.add_node(op0);
        dep_graph.add_node(op1);
        dep_graph.add_node(op2);
        dep_graph.add_node(op3);
        dep_graph.add_node(op4);

        dep_graph.add_edge(op2, op3, ());
        dep_graph.add_edge(op3, op4, ());

        // Cycle:
        //
        // 0: A remove B
        // 1: B remove C
        // 2: C delegate D
        // 3: D delegate E
        // 4: E remove A
        //
        let mut authority = AuthorityGraphs::new(dep_graph);

        authority.add_removal(group, a, b, op0);
        authority.add_removal(group, b, c, op1);
        authority.add_delegation(group, c, d, op2);
        authority.add_delegation(group, d, e, op3);
        authority.add_removal(group, e, a, op4);

        // Now the transitive cycle A -> B -> C -> A should be detected.
        assert!(authority.is_cycle(&group, &op0));
        assert!(authority.is_cycle(&group, &op1));
        assert!(authority.is_cycle(&group, &op2));
        assert!(authority.is_cycle(&group, &op3));
        assert!(authority.is_cycle(&group, &op4));
    }
}
