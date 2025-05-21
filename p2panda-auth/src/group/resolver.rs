use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::prelude::DiGraphMap;
use petgraph::visit::{Dfs, Reversed};
use thiserror::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

use super::{GroupAction, GroupStateInner};

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupResolverError {}

/// Resolver for group membership auth graph.
#[derive(Clone, Debug, Default)]
pub struct GroupResolver<ID, OP, C, ORD, GS> {
    _phantom: PhantomData<(ID, OP, C, ORD, GS)>,
}

impl<ID, OP, C, ORD, GS> Resolver<ORD::Message> for GroupResolver<ID, OP, C, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Clone + Debug,
    ORD::Message: Clone,
    ORD::State: Clone,
    GS: GroupStore<ID, OP, C, Self, ORD> + Debug,
{
    type State = GroupState<ID, OP, C, Self, ORD, GS>;
    type Error = GroupResolverError;

    fn rebuild_required(y: &Self::State, operation: &ORD::Message) -> bool {
        let control_message = operation.payload();
        let group_id = control_message.group_id();
        let _actor = operation.sender();

        // Sanity check.
        if control_message.group_id() != y.group_id {
            panic!();
        }

        let is_concurrent = !get_concurrent_operations(&y.inner.graph, operation.id()).is_empty();

        match operation.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                true
            }
            GroupControlMessage::GroupAction { .. } => {
                if is_concurrent {
                    match action {
                        GroupAction::Remove { member: _ } => {
                            // Optional optimization to avoid unnecessary re-builds, only return
                            // true if:
                            // 1) The removed member performed an admin action in any concurrent
                            //    branch && they actually were an admin.
                            // 2) ..?

                            true
                        }
                        GroupAction::Demote {
                            member: _,
                            access: _,
                        } => {
                            // Optional optimizations to avoid unnecessary re-builds, only return
                            // true if:
                            // 1) The demoted member was previously an admin && they performed an
                            //    admin action in a concurrent branch.
                            // 2) The demoted member was promoted to admin in a concurrent branch
                            //    && they performed an admin action.
                            // 3) ..?

                            true
                        }
                        _ => {
                            // TODO: Check if there are any concurrent actions which invalidate this
                            // action. If there are we could actually invalidate it immediately,
                            // maybe this method should return a state object as well as the boolean.
                            false
                        }
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Resolve group membership by processing all concurrent operations in the graph.
    ///
    /// The following ruleset is applied when chosing which operations to "filter":
    ///
    /// 1) Mutual removals
    ///
    /// In our first resolve strategy mutual removals result in both members being removed from
    /// the group. We imagine further implementations taking different approaches, like
    /// resolving by seniority, hash id, quorum or some other parameter.
    ///
    /// If a mutual removal has occurred, we want to retain the removal operations but
    /// filter all concurrent operations performed by the removed members.
    ///
    /// 2) Re-adding member concurrently
    ///
    /// We don't stop this behaviour, if Alice removes Charlie and Bob removes then adds Charlie
    /// concurrently, Charlie is still in the group.
    ///
    /// 3) Removed admin performing concurrent actions
    ///
    /// If Alice removes Bob, then Bob shouldn't be able to perform any actions concurrently.
    ///
    /// 4) Demoted admin performing concurrent actions
    ///
    /// If Alice demotes Bob (from admin), then Bob is no longer an admin and shouldn't be able to
    /// perform any actions concurrently.
    fn process(
        mut y: GroupState<ID, OP, Self, ORD, GS>,
    ) -> Result<GroupState<ID, OP, Self, ORD, GS>, Self::Error> {
        let mut filter: HashSet<OP> = Default::default();

        let bubbles = get_concurrent_bubbles(&y.inner.graph);

        for (operation_id, bubble) in bubbles {
            // TODO: Consider keeping a HashMap in memory to optimise lookup.
            let Some(operation) = y.inner.operations.iter().find(|op| op.id() == operation_id)
            else {
                // TODO: Error: Operation is expected to exist.
                panic!()
            };

            // Iterate over all concurrent operations in the bubble.
            for concurrent_operation_id in &bubble {
                let Some(concurrent_operation) = y
                    .inner
                    .operations
                    .iter()
                    .find(|op| op.id() == *concurrent_operation_id)
                else {
                    // TODO: Error: Operation is expected to exist.
                    panic!()
                };

                if let GroupControlMessage::GroupAction { action, .. } = operation.payload() {
                    // Process a remove action.
                    //
                    // Iterate over all actions that occurred concurrent to the remove and identify
                    // those authored by the removed member. Filter any action by the removed member,
                    // as long as it's 1) not a predecessor of the remove operation, and 2) not a
                    // mutual removal (removal of the remover by the removed member).
                    if let GroupAction::Remove {
                        member: removed_member,
                    } = action
                    {
                        if concurrent_operation.sender() == removed_member.id()
                            && !operation.previous().contains(concurrent_operation_id)
                        {
                            if let GroupControlMessage::GroupAction { action, .. } =
                                concurrent_operation.payload()
                            {
                                if let GroupAction::Remove { member } = action {
                                    // The removed member is concurrently removing the remover.
                                    if member.id() == operation.sender() {
                                        // Do not filter.
                                    } else {
                                        filter.insert(*concurrent_operation_id);
                                    }
                                } else {
                                    filter.insert(*concurrent_operation_id);
                                }
                            }
                        }
                    }

                    // Process a demote action.
                    //
                    // Iterate over all actions that occurred concurrent to the demote and identify
                    // those authored by the demoted member. Filter any action by the demoted member,
                    // as long as it's not a predecessor of the demote operation.
                    if let GroupAction::Demote {
                        member: demoted_member,
                        ..
                    } = action
                    {
                        if concurrent_operation.sender() == demoted_member.id()
                            && !operation.previous().contains(concurrent_operation_id)
                        {
                            if let GroupControlMessage::GroupAction { .. } =
                                concurrent_operation.payload()
                            {
                                filter.insert(*concurrent_operation_id);
                            }
                        }
                    }
                }

                // TODO(glyph): I'm not confident that the placement of this is correct.
                //
                // Process a dependent action.
                //
                // Iterate over all concurrent operations in the bubble, finding any which include
                // filtered operations in their `previous` field. Add those dependent operations to the
                // filter.
                for previous_operation in concurrent_operation.previous() {
                    if filter.contains(previous_operation) {
                        filter.insert(*concurrent_operation_id);
                    }
                }
            }
        }

        y.inner.ignore = filter;

        Ok(y)
    }
}

/// Walk the graph and identify the set of concurrent operations for each node.
fn get_concurrent_bubbles<OP>(graph: &DiGraphMap<OP, ()>) -> HashMap<OP, HashSet<OP>>
where
    OP: OperationId + Display + Ord,
{
    let mut bubbles = HashMap::new();

    graph.nodes().for_each(|target| {
        let concurrent_operations = get_concurrent_operations(graph, target);
        if !concurrent_operations.is_empty() {
            bubbles.insert(target, concurrent_operations);
        }
    });

    bubbles
}

/// Return any operations concurrent with the given target operation.
///
/// An operation is concurrent if it is not a predecessor or successor of the target operation.
fn get_concurrent_operations<OP>(graph: &DiGraphMap<OP, ()>, target: OP) -> HashSet<OP>
where
    OP: OperationId + Display + Ord,
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

#[cfg(test)]
mod tests {
    use petgraph::graph::DiGraph;

    use super::*;

    impl OperationId for &str {}

    #[test]
    fn concurrent_bubbles() {
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
        let concurrent_bubbles = get_concurrent_bubbles(&graph_map);

        assert_eq!(concurrent_bubbles.len(), 7);

        // "D": {"F", "H", "C", "E"}
        let bubble = concurrent_bubbles.get("D").unwrap();
        for id in &["F", "H", "C", "E"] {
            assert!(bubble.contains(id));
        }

        // "F": {"B", "D", "G", "E"}
        let bubble = concurrent_bubbles.get("F").unwrap();
        for id in &["B", "D", "G", "E"] {
            assert!(bubble.contains(id));
        }

        // "G": {"F", "H", "C"}
        let bubble = concurrent_bubbles.get("G").unwrap();
        for id in &["F", "H", "C"] {
            assert!(bubble.contains(id));
        }

        // "H": {"D", "E", "G", "B"}
        let bubble = concurrent_bubbles.get("H").unwrap();
        for id in &["D", "E", "G", "B"] {
            assert!(bubble.contains(id));
        }

        // "B": {"C", "F", "H"}
        let bubble = concurrent_bubbles.get("B").unwrap();
        for id in &["C", "F", "H"] {
            assert!(bubble.contains(id));
        }

        // "C": {"B", "G", "D", "E"
        let bubble = concurrent_bubbles.get("C").unwrap();
        for id in &["B", "G", "D", "E"] {
            assert!(bubble.contains(id));
        }

        // "E": {"F", "H", "D", "C"}
        let bubble = concurrent_bubbles.get("E").unwrap();
        for id in &["F", "H", "D", "C"] {
            assert!(bubble.contains(id));
        }
    }
}
