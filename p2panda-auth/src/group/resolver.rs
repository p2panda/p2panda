use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::prelude::DiGraphMap;
use petgraph::visit::{Dfs, Reversed};
use thiserror::Error;

use crate::group::{Access, GroupControlMessage, GroupState};
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

        // Sanity check.
        if y.group_id != group_id {
            // The operation is not intended for this group.
            return Err(GroupResolverError::IncorrectGroupId(group_id, y.group_id));
        }

        Ok(y.heads().into_iter().collect::<Vec<_>>() != operation.previous())
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
        let operations: HashMap<OP, ORD::Message> = y
            .operations
            .clone()
            .into_iter()
            .map(|op| (op.id(), op))
            .collect();

        let bubbles = get_concurrent_bubbles(&y.graph);

        let mut invalid_operations = HashSet::new();
        let mut mutual_removes = HashSet::new();

        for (target_operation_id, bubble) in bubbles {
            let Some(target_operation) = operations.get(&target_operation_id) else {
                return Err(GroupResolverError::MissingOperation(target_operation_id));
            };

            // Does the target operation remove or demote a manager member?
            let removed_author = y.removed_manager(target_operation);

            // If no then continue here onto the next concurrent bubble as no action is required.
            let Some(removed_author) = removed_author else {
                continue;
            };

            // Iterate over every operation in the bubble and filter out any from the author
            // removed by the target operation.
            for concurrent_operation_id in &bubble {
                let Some(concurrent_operation) = operations.get(&concurrent_operation_id) else {
                    return Err(GroupResolverError::MissingOperation(
                        *concurrent_operation_id,
                    ));
                };

                // If this concurrent operation is _not_ authored by the "target author" then we
                // can continue to the next concurrent operation without taking any action.
                if concurrent_operation.sender() != removed_author {
                    continue;
                }

                // Does this concurrent operation remove or demote an admin member?
                let concurrent_removed_admin = y.removed_manager(concurrent_operation);

                if let Some(concurrent_removed_admin) = concurrent_removed_admin {
                    // The removed member is concurrently removing the remover.
                    if concurrent_removed_admin == target_operation.sender() {
                        // We don't want to filter out mutual remove/demote operations, but we
                        // still want to filter any dependent operations for both (mutually)
                        // removed members.
                        //
                        // The "target" operations are included when collecting invalid dependent
                        // operations, we record mutual remove operations here and then remove
                        // them from the filter later.
                        mutual_removes.insert(target_operation_id);
                        mutual_removes.insert(*concurrent_operation_id);
                        continue;
                    }
                }

                // Add the concurrent operation to our filter.
                filter.insert(*concurrent_operation_id);
            }
        }

        // For all filtered operations recursively invalidate any dependent operations.
        for invalid_operation_id in &filter {
            // TODO: Error...
            let invalid_operation = operations.get(invalid_operation_id).unwrap();
            y.invalid_dependent_operations(
                &operations,
                *invalid_operation_id,
                invalid_operation.sender(),
                &mut invalid_operations,
            );
        }

        filter.extend(invalid_operations);
        filter.retain(|op| !mutual_removes.contains(op));

        y.ignore = filter;

        Ok(y)
    }
}

impl<ID, OP, C, RS, ORD, GS> GroupState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ORD::Message> + Debug,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Debug,
    GS: GroupStore<ID, Group = GroupState<ID, OP, C, RS, ORD, GS>> + Debug,
{
    fn removed_manager(&self, operation: &ORD::Message) -> Option<ID> {
        let GroupControlMessage::GroupAction { action, .. } = operation.payload() else {
            // Revoke operations not yet supported.
            unimplemented!()
        };

        let removed_or_demoted_member = match action {
            GroupAction::Remove { member } => member,
            GroupAction::Demote { member, ref access } => {
                // If the demoted access level is still "manage" then the manager was not removed.
                if access == &Access::Manage {
                    return None;
                };
                member
            }
            _ => return None,
        };

        // We only need to react to a filtered demote operation if the target author
        // did have admin access but now doesn't.
        let was_manager = self
            .state_at(&HashSet::from_iter(operation.previous()))
            .expect("state exists for all operations")
            .managers()
            .contains(&removed_or_demoted_member);

        if was_manager {
            Some(removed_or_demoted_member.id())
        } else {
            None
        }
    }

    fn added_manager(&self, operation: &ORD::Message) -> Option<(OP, ID)> {
        let GroupControlMessage::GroupAction { action, .. } = operation.payload() else {
            // Revoke operations not yet supported.
            unimplemented!()
        };

        let (added_or_promoted_to_manager, access) = match action {
            GroupAction::Add { member, access } => (member, access),
            GroupAction::Promote { member, access } => (member, access),
            _ => return None,
        };

        // We only need to react to a members which were _not_ managers but now are.
        let was_manager = self
            .state_at(&HashSet::from_iter(operation.previous()))
            .expect("state exists for all operations")
            .managers()
            .contains(&added_or_promoted_to_manager);

        if !was_manager && access == Access::Manage {
            Some((operation.id(), added_or_promoted_to_manager.id()))
        } else {
            None
        }
    }

    // TODO: Document properly.
    //
    // TODO: Before invalidating dependent operations we need to check that the affected member has not
    // been readded by another member concurrently or otherwise. Implement this later...
    //
    // When we find an invalid operation we want to find any dependent operations and see if any of
    // those are now invalid. We need to do this recursively until no more dependent operations are
    // found.
    //
    // We do this once for every filtered operation.
    //
    // Dependent operations are all successors of the target operation.
    fn invalid_dependent_operations(
        &self,
        operations: &HashMap<OP, ORD::Message>,
        target: OP,
        target_author: ID,
        invalid_operations: &mut HashSet<OP>,
    ) {
        let mut dfs = Dfs::new(&self.graph, target);
        while let Some(dependent_operation_id) = dfs.next(&self.graph) {
            let dependent_operation = operations.get(&dependent_operation_id).unwrap();

            if dependent_operation.sender() != target_author {
                //    // TODO: if this operation is someone else adding back the target author then //
                //    break out of the search as we don't want to invalidate any more operations.
                //
                //    if let Some((_, added_manager)) = self.added_manager(dependent_operation) { if
                //        added_manager == target_author { break; } }

                continue;
            }

            invalid_operations.insert(dependent_operation_id);

            if let Some((operation_id, added_manager)) = self.added_manager(dependent_operation) {
                self.invalid_dependent_operations(
                    operations,
                    operation_id,
                    added_manager,
                    invalid_operations,
                );
            }
        }
    }
}

/// Walk the graph and identify the set of concurrent operations for each node.
// TODO: Consider removing the `get_` prefix.
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
// TODO: Consider removing the `get_` prefix.
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

    use crate::group::test_utils::{
        Network, TestGroup, TestGroupState, TestGroupStore, TestOrdererState,
    };
    use crate::group::{Access, GroupAction, GroupControlMessage, GroupMember};
    use crate::traits::{AuthGroup, OperationId};

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

        // We expect Alice (Manage), Bob (Write) and Claire (Manage) to be the only group members.
        let expected_members = vec![
            (GroupMember::Individual(alice), Access::Manage),
            (
                GroupMember::Individual(bob),
                Access::Write { conditions: None },
            ),
            (GroupMember::Individual(claire), Access::Manage),
        ];

        let alice_members = network.members(&alice, &group);
        assert_eq!(alice_members, expected_members);

        let bob_members = network.members(&bob, &group);
        assert_eq!(bob_members, expected_members);

        let claire_members = network.members(&claire, &group);
        assert_eq!(claire_members, expected_members);

        // We expect each filter to contain a single operation: the addition of Dave.
        let alice_filter = network.get_y(&alice, &group).ignore;
        assert_eq!(alice_filter.len(), 1);

        let bob_filter = network.get_y(&bob, &group).ignore;
        assert_eq!(bob_filter.len(), 1);

        let claire_filter = network.get_y(&claire, &group).ignore;
        assert_eq!(claire_filter.len(), 1);
    }

    #[test]
    fn remove_dependencies_filter() {
        //       A
        //     /   \
        //    B     C
        //           \
        //            D
        //
        // Node A: create the group with Alice and Bob as managers
        // Node B: Alice removes Bob
        // Node C: Bob adds Claire
        // Node D: Claire adds Dave
        //
        // We expect the addition of Claire (node C) and Dave (node D) to be filtered.
        // Alice should be the only member of the group after processing.

        let group_id = '1';

        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';

        let mut rng = StdRng::from_os_rng();

        let alice_store = TestGroupStore::default();
        let alice_orderer_y =
            TestOrdererState::new(alice, alice_store.clone(), StdRng::from_rng(&mut rng));
        // TODO: Do we maybe want to switch the position of the args `alice` and `group_id`?
        let alice_group_y = TestGroupState::new(alice, group_id, alice_store, alice_orderer_y);

        let bob_store = TestGroupStore::default();
        let bob_orderer_y =
            TestOrdererState::new(bob, bob_store.clone(), StdRng::from_rng(&mut rng));
        let bob_group_y = TestGroupState::new(bob, group_id, bob_store, bob_orderer_y);

        let claire_store = TestGroupStore::default();
        let claire_orderer_y =
            TestOrdererState::new(claire, claire_store.clone(), StdRng::from_rng(&mut rng));
        let claire_group_y = TestGroupState::new(claire, group_id, claire_store, claire_orderer_y);

        // Create group with alice and bob as initial admin members.
        let control_message_001 = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Individual(alice), Access::Manage),
                    (GroupMember::Individual(bob), Access::Manage),
                ],
            },
        };
        let (alice_group_y, operation_001) =
            TestGroup::prepare(alice_group_y, &control_message_001).unwrap();
        let alice_group_y = TestGroup::process(alice_group_y, &operation_001).unwrap();
        let bob_group_y = TestGroup::process(bob_group_y, &operation_001).unwrap();
        let claire_group_y = TestGroup::process(claire_group_y, &operation_001).unwrap();

        let mut members = alice_group_y.members();
        members.sort();
        let expected_members = vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Manage),
        ];
        assert_eq!(members, expected_members);

        // Alice removes Bob.
        let control_message_002 = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(bob),
            },
        };

        let (alice_group_y, operation_002) =
            TestGroup::prepare(alice_group_y, &control_message_002).unwrap();
        // Only Alice processes this operation.
        let alice_group_y = TestGroup::process(alice_group_y, &operation_002).unwrap();

        let mut members = alice_group_y.members();
        members.sort();
        let expected_members = vec![(GroupMember::Individual(alice), Access::Manage)];
        assert_eq!(members, expected_members);

        // Bob adds claire with manage access.
        let control_message_003 = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(claire),
                access: Access::Manage,
            },
        };
        let (bob_group_y, operation_003) =
            TestGroup::prepare(bob_group_y, &control_message_003).unwrap();
        let bob_group_y = TestGroup::process(bob_group_y, &operation_003).unwrap();
        let claire_group_y = TestGroup::process(claire_group_y, &operation_003).unwrap();

        let mut members = bob_group_y.members();
        members.sort();
        let expected_members = vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Manage),
            (GroupMember::Individual(claire), Access::Manage),
        ];
        assert_eq!(members, expected_members);

        // Claire adds Dave with read access.
        let control_message_004 = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::Read,
            },
        };
        let (claire_group_y, operation_004) =
            TestGroup::prepare(claire_group_y, &control_message_004).unwrap();
        let claire_group_y = TestGroup::process(claire_group_y, &operation_004).unwrap();
        let bob_group_y = TestGroup::process(bob_group_y, &operation_004).unwrap();

        let mut members = bob_group_y.members();
        members.sort();
        let expected_members = vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Manage),
            (GroupMember::Individual(claire), Access::Manage),
            (GroupMember::Individual(dave), Access::Read),
        ];
        assert_eq!(members, expected_members);

        // Now everyone processes the operations from the concurrent branch.
        let alice_group_y = TestGroup::process(alice_group_y, &operation_003).unwrap();
        TestGroup::process(alice_group_y, &operation_004).unwrap();
        TestGroup::process(bob_group_y, &operation_002).unwrap();
        TestGroup::process(claire_group_y, &operation_002).unwrap();
    }
}
