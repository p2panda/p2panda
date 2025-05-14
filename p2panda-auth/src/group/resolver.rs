use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::prelude::DiGraphMap;
use petgraph::visit::{Dfs, Reversed};
use thiserror::Error;

use crate::group::{GroupControlMessage, GroupMember, GroupState};
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
        let actor = operation.sender();

        // Sanity check.
        if control_message.group_id() != y.group_id {
            panic!();
        }

        // Get all current tip operations.
        //
        // TODO: should be checking against transitive heads here.
        let heads = y.heads();

        // Detect concurrent operations by comparing the current heads with the new operations'
        // dependencies.
        let is_concurrent = heads != HashSet::from_iter(operation.dependencies().clone());

        // TODO: Get concurrent branches.
        // let concurrent_operations = get_concurrent(operation: &OP, graph: &y.inner.graph);

        match operation.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                true
            }
            GroupControlMessage::GroupAction { .. } => {
                if is_concurrent {
                    match action {
                        GroupAction::Remove { member } => {
                            // Optional optimization to avoid unnecessary re-builds, only return
                            // true if:
                            // 1) The removed member performed an admin action in any concurrent
                            //    branch && they actually were an admin.
                            // 2) ..?

                            true
                        }
                        GroupAction::Demote { member, access } => {
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

    fn process(
        mut y: GroupState<ID, OP, Self, ORD, GS>,
    ) -> Result<GroupState<ID, OP, Self, ORD, GS>, Self::Error> {
        // All bubbles present in this graph.
        //
        // TODO: Conversion between `DiGraphMap` and `DiGraph` (or better solution).
        let bubbles = get_concurrent_bubbles(&y.inner.graph);

        // A new set of operations to be filtered which we will now populate.
        let mut filter: HashSet<OP> = Default::default();

        // Iterate over all bubbles, apply membership rules and populate the filter accordingly.
        for (operation, bubble) in bubbles {
            // Steps based on auth membership rules: https://github.com/local-first-web/auth/blob/f61e3678d74f9a30946475941ef9ef0c8c45d664/packages/auth/src/team/membershipResolver.ts#L83
            //
            // NOTE: we made some different decisions about how to resolve conflicts, but
            // how to understand what constitutes a conflict is still useful to follow.

            // 1) Mutual removals
            //
            // In our first resolve strategy mutual removals result in both members being removed from
            // the group. We imagine further implementations taking different approaches, like
            // resolving by seniority, hash id, quorum or some other parameter.

            // Is `operation` a removal?
            // - Who performed the removal?
            // - Does any operation in the `bubble` remove the remover?
            //   - If so, add both to the filter
            //   - Also add all concurrent operations performed by remover and removed

            // 2) Re-adding member concurrently
            //
            // We don't stop this behaviour, if A removes C and B removes then adds C concurrently, C is still
            // in the group.

            // 3) Removed admin performing concurrent actions
            //
            // If A removes B, then B shouldn't be able to perform any actions concurrently.

            if let GroupControlMessage::GroupAction { action, .. } = operation.payload() {
                if let GroupAction::Remove { member } = action {
                    for op in bubble {
                        if op.sender() == member {
                            filter.insert(op);
                        };
                    }
                }
            }

            // 4) Demoted admin performing concurrent actions
            //
            // If A demotes B (from admin), then B shouldn't be able to perform any actions concurrently.

            // Is `operation` a demotion from admin?
            // - Filter all concurrent operations performed by the demoted actor.
        }

        // Set the new "ignore filter".
        y.inner.ignore = filter;

        Ok(y)
    }
}

// Returns a HashMap containing a hash and all hashes directly or indirectly concurrent with it.
fn get_concurrent_bubbles<OP>(graph: &DiGraph<OP, ()>) -> HashMap<OP, HashSet<OP>> {
    let mut bubbles = HashMap::new();

    // Walk the graph.
    graph.node_indices().for_each(|target| {
        // Get all concurrent operations for this node.
        let concurrent_operations = get_concurrent_operations(graph, target);
        if !concurrent_operations.is_empty() {
            bubbles.insert(target, concurrent_operations);
        }
    });

    bubbles
}

// Return concurrent operations for a given target node / operation.
fn get_concurrent_operations<OP>(graph: &DiGraph<OP, ()>, target: NodeIndex) -> HashSet<NodeIndex> {
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
    graph
        .node_indices()
        .filter(|n| !relatives.contains(n))
        .collect()
}
