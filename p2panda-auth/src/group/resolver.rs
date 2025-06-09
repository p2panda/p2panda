use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::prelude::DiGraphMap;
use petgraph::visit::{Dfs, Reversed};
use thiserror::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

use super::GroupAction;

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupResolverError<ID, OP>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
{
    #[error("operation id {0} exists in the graph but the corresponding operation was not found")]
    MissingOperation(OP),

    #[error("operation for group {0} processed in group {1}")]
    IncorrectGroupId(ID, ID),
}

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
    type Error = GroupResolverError<ID, OP>;

    fn rebuild_required(
        y: &GroupState<ID, OP, C, Self, ORD, GS>,
        operation: &ORD::Message,
    ) -> Result<bool, GroupResolverError<ID, OP>> {
        let control_message = operation.payload();
        let group_id = control_message.group_id();
        let _actor = operation.sender();

        // Sanity check.
        if y.group_id != group_id {
            // The operation is not intended for this group.
            return Err(GroupResolverError::IncorrectGroupId(group_id, y.group_id));
        }

        let is_concurrent = !get_concurrent_operations(&y.graph, operation.id()).is_empty();

        match operation.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                Ok(true)
            }
            GroupControlMessage::GroupAction { action, .. } => {
                if is_concurrent {
                    match action {
                        GroupAction::Remove { member: _ } => {
                            // Optional optimization to avoid unnecessary re-builds, only return
                            // true if:
                            // 1) The removed member performed an admin action in any concurrent
                            //    branch && they actually were an admin.
                            // 2) ..?

                            Ok(true)
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

                            Ok(true)
                        }
                        _ => {
                            // TODO: Check if there are any concurrent actions which invalidate this
                            // action. If there are we could actually invalidate it immediately,
                            // maybe this method should return a state object as well as the boolean.
                            Ok(false)
                        }
                    }
                } else {
                    Ok(false)
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
        mut y: GroupState<ID, OP, C, Self, ORD, GS>,
    ) -> Result<GroupState<ID, OP, C, Self, ORD, GS>, Self::Error> {
        let mut filter: HashSet<OP> = Default::default();

        let bubbles = get_concurrent_bubbles(&y.graph);
        println!("bubbles: {:?}", bubbles);

        for (operation_id, bubble) in bubbles {
            let Some(operation) = y.operations.iter().find(|op| op.id() == operation_id) else {
                return Err(GroupResolverError::MissingOperation(operation_id));
            };

            // Iterate over all concurrent operations in the bubble.
            for concurrent_operation_id in &bubble {
                let Some(concurrent_operation) = y
                    .operations
                    .iter()
                    .find(|op| op.id() == *concurrent_operation_id)
                else {
                    return Err(GroupResolverError::MissingOperation(
                        *concurrent_operation_id,
                    ));
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
                        println!("processing removal of {:?}", removed_member);

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
                                        println!("removed member is concurrently removing remover; no filtering");
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
                        println!("processing demotion of {:?}", demoted_member);

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
                    if filter.contains(&previous_operation) {
                        println!("filtering a dependent action");
                        filter.insert(*concurrent_operation_id);
                    }
                }
            }
        }

        y.ignore = filter;

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
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use petgraph::graph::DiGraph;
    use petgraph::prelude::DiGraphMap;

    use crate::group::test_utils::Network;
    use crate::group::{Access, GroupMember};
    use crate::traits::OperationId;

    use super::get_concurrent_bubbles;

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

    #[test]
    fn mutual_removal_filter() {
        //       A
        //     /   \
        //    B     C
        //
        // Node A: create the group
        // Node B: Alice removes Bob
        // Node C: Bob removes Alice
        //
        // We expect both removals to be processed (not filtered).

        let alice = 'A';
        let bob = 'B';
        let claire = 'C';

        let group = '1';

        let rng = StdRng::from_os_rng();

        let mut network = Network::new([alice, bob, claire], rng);

        // Alice creates a group with Alice, Bob and Claire as managers.
        network.create(
            group,
            alice,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // Everyone processes the operation.
        network.process();

        // Alice and Bob remove one another concurrently.
        network.remove(alice, GroupMember::Individual(bob), group);
        network.remove(bob, GroupMember::Individual(alice), group);

        // Everyone processes these operations.
        network.process();

        // We expect Claire to be the only remaining group member.
        let alice_members = network.members(&alice, &group);
        assert_eq!(
            alice_members,
            vec![(GroupMember::Individual(claire), Access::Manage),]
        );

        let bob_members = network.members(&bob, &group);
        assert_eq!(
            bob_members,
            vec![(GroupMember::Individual(claire), Access::Manage),]
        );

        let claire_members = network.members(&claire, &group);
        assert_eq!(
            claire_members,
            vec![(GroupMember::Individual(claire), Access::Manage),]
        );

        // We expect the "ignore" operation set to be empty, indicating that no operations have
        // been marked as invalid by the resolver.
        let alice_filter = network.get_y(&alice, &group).ignore;
        assert!(alice_filter.is_empty());

        let bob_filter = network.get_y(&bob, &group).ignore;
        assert!(bob_filter.is_empty());

        let claire_filter = network.get_y(&claire, &group).ignore;
        assert!(claire_filter.is_empty());
    }

    #[test]
    fn demote_remove_filter() {
        //       A
        //     /   \
        //    B     C
        //
        // Node A: create the group
        // Node B: Alice demotes Bob to Write
        // Node C: Bob removes Claire
        //
        // We expect the removal of Claire (node C) to be filtered.

        let alice = 'A';
        let bob = 'B';
        let claire = 'C';

        let group = '1';

        let rng = StdRng::from_os_rng();

        let mut network = Network::new([alice, bob, claire], rng);

        // Alice creates a group with Alice, Bob and Claire as managers.
        network.create(
            group,
            alice,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // Everyone processes the operation.
        network.process();

        // Alice demotes Bob.
        network.demote(
            alice,
            GroupMember::Individual(bob),
            group,
            Access::Write { conditions: None },
        );

        // Bob removes Claire concurrently.
        network.remove(bob, GroupMember::Individual(claire), group);

        // Everyone processes these operations.
        network.process();

        // The demote operation should have been applied.
        // The remove operation should have been filtered.

        // TODO: Assertions fail.
        // Bob has the expected membership state but Alice and Claire do not.

        // We expect Alice (Manage), Bob (Write) and Claire (Manage) to be the only group members.
        let alice_members = network.members(&alice, &group);
        assert_eq!(
            alice_members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (
                    GroupMember::Individual(bob),
                    Access::Write { conditions: None }
                ),
                (GroupMember::Individual(claire), Access::Manage),
            ]
        );

        let bob_members = network.members(&bob, &group);
        assert_eq!(
            bob_members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (
                    GroupMember::Individual(bob),
                    Access::Write { conditions: None }
                ),
                (GroupMember::Individual(claire), Access::Manage),
            ]
        );

        let claire_members = network.members(&claire, &group);
        assert_eq!(
            claire_members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (
                    GroupMember::Individual(bob),
                    Access::Write { conditions: None }
                ),
                (GroupMember::Individual(claire), Access::Manage),
            ]
        );

        // We expect the "ignore" operation set to contain a single operation ID.
        // The ID should be the same across all members.
        let alice_filter = network.get_y(&alice, &group).ignore;
        let bob_filter = network.get_y(&bob, &group).ignore;
        let claire_filter = network.get_y(&claire, &group).ignore;
        assert_eq!(alice_filter.len(), 1);
        assert_eq!(alice_filter, bob_filter);
        assert_eq!(bob_filter, claire_filter);
    }

    #[test]
    fn demote_add_filter() {
        //       A
        //     /   \
        //    B     C
        //
        // Node A: create the group
        // Node B: Alice demotes Bob to Write
        // Node C: Bob adds Dave
        //
        // We expect the addition of Dave (node C) to be filtered.

        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';

        let group = '1';

        let rng = StdRng::from_os_rng();

        let mut network = Network::new([alice, bob, claire, dave], rng);

        // Alice creates a group with Alice, Bob and Claire as managers.
        network.create(
            group,
            alice,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // Everyone processes the operation.
        network.process();

        // Alice demotes Bob.
        network.demote(
            alice,
            GroupMember::Individual(bob),
            group,
            Access::Write { conditions: None },
        );

        // Bob adds Dave concurrently.
        network.add(bob, GroupMember::Individual(dave), group, Access::Read);

        // Everyone processes these operations.
        network.process();

        // The demote operation should have been applied.
        // The add operation should have been filtered.

        // TODO: Assertions fail.
        // Bob has the expected membership state but Alice and Claire do not.

        // We expect Alice (Manage), Bob (Write) and Claire (Manage) to be the only group members.
        let alice_members = network.members(&alice, &group);
        assert_eq!(
            alice_members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (
                    GroupMember::Individual(bob),
                    Access::Write { conditions: None }
                ),
                (GroupMember::Individual(claire), Access::Manage),
            ]
        );

        let bob_members = network.members(&bob, &group);
        assert_eq!(
            bob_members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (
                    GroupMember::Individual(bob),
                    Access::Write { conditions: None }
                ),
                (GroupMember::Individual(claire), Access::Manage),
            ]
        );

        let claire_members = network.members(&claire, &group);
        assert_eq!(
            claire_members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (
                    GroupMember::Individual(bob),
                    Access::Write { conditions: None }
                ),
                (GroupMember::Individual(claire), Access::Manage),
            ]
        );

        // TODO: Assertions fail.
        // Bob has one operation in the filter but Alice and Claire do not.
        // Bob has successfully filtered the self-authored operation which adds Dave.
        //
        // The concurrent bubbles are somehow not being recognised by Alice and Claire
        // (`get_concurrent_bubbles()` returns an empty HashMap).

        // We expect the "ignore" operation set to be empty, indicating that no operations have
        // been marked as invalid by the resolver.
        let alice_filter = network.get_y(&alice, &group).ignore;
        assert!(alice_filter.is_empty());

        let bob_filter = network.get_y(&bob, &group).ignore;
        assert!(bob_filter.is_empty());

        let claire_filter = network.get_y(&claire, &group).ignore;
        assert!(claire_filter.is_empty());
    }

    #[test]
    fn remove_dependencies_filter() {
        //       A
        //     /   \
        //    B     C
        //           \
        //            D
        //
        // Node A: create the group
        // Node B: Alice removes Bob
        // Node C: Bob adds Claire
        // Node D: Claire adds Dave
        //
        // We expect the addition of Claire (node C) and Dave (node D) to be filtered.
        // Alice should be the only member of the group after processing.

        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';

        let group = '1';

        let rng = StdRng::from_os_rng();

        let mut network = Network::new([alice, bob, claire, dave], rng);

        // Alice creates a group with Alice and Bob as managers.
        network.create(
            group,
            alice,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
            ],
        );

        // Everyone processes the operation.
        network.process();

        // Alice removes Bob.
        network.remove(alice, GroupMember::Individual(bob), group);

        // Bob adds Claire concurrently.
        network.add(bob, GroupMember::Individual(claire), group, Access::Manage);

        // Claire adds Dave concurrently.
        network.add(claire, GroupMember::Individual(dave), group, Access::Manage);

        // Everyone processes these operations.
        network.process();

        // We expect Alice to be the only remaining group member.
        let alice_members = network.members(&alice, &group);
        assert_eq!(
            alice_members,
            vec![(GroupMember::Individual(alice), Access::Manage),]
        );

        let bob_members = network.members(&bob, &group);
        assert_eq!(
            bob_members,
            vec![(GroupMember::Individual(alice), Access::Manage),]
        );

        let claire_members = network.members(&claire, &group);
        assert_eq!(
            claire_members,
            vec![(GroupMember::Individual(alice), Access::Manage),]
        );
    }
}
