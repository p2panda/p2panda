// SPDX-License-Identifier: MIT OR Apache-2.0

//! Graph functions for identifying related sets of concurrent operations.

use std::collections::HashSet;

use petgraph::graphmap::DiGraphMap;
use petgraph::visit::{Dfs, Reversed};

use crate::traits::OperationId;

/// Recursively identify all operations concurrent with the given target operation.
fn concurrent_bubble<OP>(
    graph: &DiGraphMap<OP, ()>,
    target: OP,
    processed: &mut HashSet<OP>,
) -> HashSet<OP>
where
    OP: OperationId + Ord,
{
    let mut bubble = HashSet::new();
    bubble.insert(target);

    concurrent_operations(graph, target)
        .into_iter()
        .for_each(|op| {
            if processed.insert(op) {
                bubble.extend(concurrent_bubble(graph, op, processed).iter())
            }
        });

    bubble
}

/// Walk the graph and identify all sets of concurrent operations.
pub fn concurrent_bubbles<OP>(graph: &DiGraphMap<OP, ()>) -> Vec<HashSet<OP>>
where
    OP: OperationId + Ord,
{
    let mut processed: HashSet<OP> = HashSet::new();
    let mut bubbles = Vec::new();

    graph.nodes().for_each(|target| {
        if processed.insert(target) {
            let bubble = concurrent_bubble(graph, target, &mut processed);
            if bubble.len() > 1 {
                bubbles.push(bubble)
            }
        }
    });

    bubbles
}

/// Return any operations concurrent with the given target operation.
///
/// Operations are considered concurrent if they are neither predecessors nor successors of the
/// target operation.
fn concurrent_operations<OP>(graph: &DiGraphMap<OP, ()>, target: OP) -> HashSet<OP>
where
    OP: OperationId + Ord,
{
    // Get all successors.
    let mut successors = HashSet::new();
    let mut dfs = Dfs::new(&graph, target);
    while let Some(nx) = dfs.next(&graph) {
        successors.insert(nx);
    }

    // Get all predecessors.
    let mut predecessors = HashSet::new();
    let reversed = Reversed(graph);
    let mut dfs_rev = Dfs::new(&reversed, target);
    while let Some(nx) = dfs_rev.next(&reversed) {
        predecessors.insert(nx);
    }

    let relatives: HashSet<_> = successors.union(&predecessors).cloned().collect();

    // Collect all operations which are not successors or predecessors.
    graph.nodes().filter(|n| !relatives.contains(n)).collect()
}

/// Return `true` if a linear path exists in the graph between `from` and `to`.
///
/// This indicates whether or not the given operations occurred concurrently.
pub fn has_path<OP>(graph: &DiGraphMap<OP, ()>, from: OP, to: OP) -> bool
where
    OP: OperationId + Ord,
{
    let mut dfs = Dfs::new(graph, from);
    while let Some(node) = dfs.next(graph) {
        if node == to {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use petgraph::{graph::DiGraph, prelude::DiGraphMap};

    use crate::graph::concurrent_bubbles;

    #[test]
    fn test_linear_chain_no_concurrency() {
        let mut graph = DiGraphMap::new();
        graph.add_edge(1, 2, ());
        graph.add_edge(2, 3, ());
        graph.add_edge(3, 4, ());

        let bubbles = concurrent_bubbles(&graph);
        assert!(bubbles.is_empty());
    }

    #[test]
    fn test_bubble() {
        let mut graph = DiGraphMap::new();
        graph.add_edge(1, 2, ());
        graph.add_edge(1, 3, ());
        graph.add_edge(2, 4, ());
        graph.add_edge(3, 4, ());

        let bubbles = concurrent_bubbles(&graph);

        // 2 and 3 are concurrent.
        assert_eq!(bubbles.len(), 1);
        let expected: HashSet<_> = [2, 3].into_iter().collect();
        assert_eq!(bubbles[0], expected);
    }

    #[test]
    fn test_two_bubbles() {
        let mut graph = DiGraphMap::new();
        // Bubble 1: 1 → 2, 1 → 3, 2 → 4, 3 → 4
        graph.add_edge(1, 2, ());
        graph.add_edge(1, 3, ());
        graph.add_edge(2, 4, ());
        graph.add_edge(3, 4, ());
        // Bubble 2: 4 → 5, 4 → 6, 5 → 7, 6 → 7
        graph.add_edge(4, 5, ());
        graph.add_edge(4, 6, ());
        graph.add_edge(5, 7, ());
        graph.add_edge(6, 7, ());

        let bubbles = concurrent_bubbles(&graph);
        assert_eq!(bubbles.len(), 2);

        let b1: HashSet<_> = [2, 3].into_iter().collect();
        let b2: HashSet<_> = [5, 6].into_iter().collect();

        assert!(bubbles.contains(&b1));
        assert!(bubbles.contains(&b2));
    }

    #[test]
    fn complex_bubble() {
        //       A
        //     /   \
        //    B     C
        //   / \     \
        //  D   E     F
        //   \ /     /
        //    G     H
        //     \   /
        //       I
        //       |
        //       J

        let mut graph = DiGraph::new();

        // Add nodes A–M.
        let a = graph.add_node("A"); // 0
        let b = graph.add_node("B"); // 1
        let c = graph.add_node("C"); // 2
        let d = graph.add_node("D"); // 3
        let e = graph.add_node("E"); // 4
        let f = graph.add_node("F"); // 5
        let g = graph.add_node("G"); // 6
        let h = graph.add_node("H"); // 7
        let i = graph.add_node("I"); // 8
        let j = graph.add_node("J"); // 9

        // Add edges.
        graph.extend_with_edges(&[
            (a, b),
            (a, c),
            (b, d),
            (b, e),
            (d, g),
            (e, g),
            (c, f),
            (f, h),
            (h, i),
            (g, i),
            (i, j),
        ]);

        let graph_map = DiGraphMap::from_graph(graph);
        let concurrent_bubbles = concurrent_bubbles(&graph_map);

        assert_eq!(concurrent_bubbles.len(), 1);
        let bubble = concurrent_bubbles.first().unwrap();
        for id in &["B", "C", "D", "E", "F", "G", "H"] {
            assert!(bubble.contains(id));
        }
    }
}
