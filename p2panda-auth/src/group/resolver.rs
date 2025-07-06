// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strong remove group resolver implementation.
use std::collections::HashSet;
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::algo::toposort;

use crate::Access;
use crate::graph::{concurrent_bubbles, has_path};
use crate::group::{
    GroupAction, GroupControlMessage, GroupCrdt, GroupCrdtError, GroupCrdtState, StateChangeResult,
};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Orderer, Resolver};

/// An implementation of `GroupResolver` trait which follows strong remove ruleset.  
///
/// Concurrent operations are identified and processed, any which should be invalidated are
/// added to the operation filter and not applied to the group state. Once an operation has
/// been filtered, any operations which depended on any resulting state will not be applied to
/// group state either. Ruleset for Concurrent Operations
///
/// The following ruleset is applied when choosing which operations to "filter" when concurrent
/// operations are processed. It can be assumed that the behavior is equivalent for an admin
/// member being removed, or demoted from admin to a lower access level.
///
/// ## Strong Remove Concurrency Rules
///
/// ### Removals
///
/// If a removal has occurred, filter any concurrent operations by the removed member, as long
/// as it's 1) not a predecessor of the remove operation, and 2) not a mutual removal (removal
/// of the remover by the removed member).
///
/// ### Mutual removals
///
/// Mutual removals result in both members being removed from the group, and any dependent
/// concurrent branches are not applied to group state. We imagine further implementations
/// taking different approaches, like resolving by seniority, hash id, quorum or some other
/// parameter.
///
/// If a mutual removal has occurred, we want to retain the removal operations but filter all
/// concurrent operations performed by the removed members (keeping predecessors of the
/// remove).
///
/// ### Re-adding member concurrently
///
/// If Alice removes Charlie and Bob removes then adds Charlie concurrently, Charlie is still
/// in the group. However, if Charlie performed any concurrent actions, these will be filtered
/// along with any dependent operations.
///
/// ### Filtering of transitively dependent operations
///
/// When an operation is "explicitly" filtered it may cause dependent operations to become
/// invalid, these operations will not be applied to the group state.
#[derive(Clone, Debug, Default)]
pub struct StrongRemove<ID, OP, C, ORD, GS> {
    _phantom: PhantomData<(ID, OP, C, ORD, GS)>,
}

impl<ID, OP, C, ORD, GS> Resolver<ID, OP, C, ORD, GS> for StrongRemove<ID, OP, C, ORD, GS>
where
    ID: IdentityHandle + Display + Ord,
    OP: OperationId + Display + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Clone + Debug,
    ORD::Operation: Clone,
    ORD::State: Clone,
    GS: GroupStore<ID, OP, C, Self, ORD> + Debug + Clone,
{
    /// Identify if an operation should trigger a group state rebuild.
    fn rebuild_required(
        y: &GroupCrdtState<ID, OP, C, Self, ORD, GS>,
        operation: &ORD::Operation,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, Self, ORD, GS>> {
        let control_message = operation.payload();
        let group_id = control_message.group_id();

        // Sanity check.
        if y.group_id != group_id {
            // The operation is not intended for this group.
            return Err(GroupCrdtError::IncorrectGroupId(group_id, y.group_id));
        }

        let transitive_heads = y.transitive_heads().unwrap();

        // We only need to rebuild the graph if this operation is concurrent.
        Ok(transitive_heads.into_iter().collect::<Vec<_>>() != operation.dependencies())
    }

    /// Process the group operation graph, producing a new filter and re-building all state
    /// accordingly.
    fn process(
        mut y: GroupCrdtState<ID, OP, C, Self, ORD, GS>,
    ) -> Result<GroupCrdtState<ID, OP, C, Self, ORD, GS>, GroupCrdtError<ID, OP, C, Self, ORD, GS>>
    {
        // Start by draining the existing filter and re-building all states.
        y.ignore.drain();
        let mut y = GroupCrdt::rebuild(y).expect("no errors when re-building a group");
        let mut filter: HashSet<OP> = Default::default();

        // @TODO: avoid cloning operations here.
        let operations = y.operations.clone();

        // Keep track of mutual removes (which occur in one bubble) so that we can exclude these
        // from the filter later.
        let mut mutual_removes = HashSet::new();

        // Get all bubbles of concurrency.
        //
        // A concurrency bubble is a set of operations from the group graph which share some
        // concurrency. Multiple bubbles can occur in the same graph.
        let mut bubbles = concurrent_bubbles(&y.graph);

        let topo_sort =
            toposort(&y.graph, None).expect("group operation sets can be ordered topologically");
        let mut visited = HashSet::new();

        // Traverse the graph visiting the operations in topological order.
        for target_operation_id in topo_sort.iter() {
            let Some(target_operation) = operations.get(target_operation_id) else {
                return Err(GroupCrdtError::MissingOperation(*target_operation_id));
            };

            let bubble = bubbles
                .iter()
                .find(|bubble| bubble.contains(target_operation_id))
                .cloned();

            visited.insert(*target_operation_id);

            let removed_manager = y.removed_manager(target_operation);

            // If this operation removes/demotes a member with manager rights _and_ it is part of
            // a concurrency bubble then we need to check the bubble for any operations which
            // should be filtered.
            if let (Some(removed_manager), Some(bubble)) = (removed_manager, &bubble) {
                for bubble_operation_id in bubble.iter() {
                    // If there's a path between the bubble and target operation, then it's not
                    // concurrent, so we don't need to do anything.
                    if has_path(&y.graph, *bubble_operation_id, *target_operation_id) {
                        continue;
                    }

                    let Some(bubble_operation) = operations.get(bubble_operation_id) else {
                        return Err(GroupCrdtError::MissingOperation(*bubble_operation_id));
                    };

                    // If this concurrent operation is _not_ authored by the "target author" then we
                    // can continue to the next concurrent operation without taking any action.
                    if bubble_operation.author() != removed_manager {
                        continue;
                    }

                    // Add the concurrent operation to our filter.
                    filter.insert(*bubble_operation_id);

                    // Does this concurrent operation remove or demote an admin member?
                    if let Some(concurrent_removed_admin) = y.removed_manager(bubble_operation) {
                        // The removed member is concurrently removing the remover, this
                        // is a "mutual remove".
                        if concurrent_removed_admin == target_operation.author() {
                            mutual_removes.insert(*bubble_operation_id);
                        }
                    }
                }
            }

            match bubble {
                Some(bubble) => {
                    // If "bubble" is a sub-set of "visited" then we have visited all operations
                    // in this bubble and we should now re-calculate group state using the newly
                    // produced filter.
                    //
                    // This step is required so that as we traverse further into the graph we're
                    // making decisions based on state computed _after_ we resolved any earlier
                    // bubbles.
                    if bubble.is_subset(&visited) {
                        let mut filter_tmp = filter.clone();
                        filter_tmp.retain(|op: &OP| !mutual_removes.contains(op));
                        y.ignore = filter_tmp;
                        y = GroupCrdt::rebuild(y).expect("no errors when re-building a group");
                        // Remove the visited bubble from the bubbles set.
                        bubbles.retain(|b| *b != bubble);

                        // Drain visited.
                        visited.drain();
                    }
                }
                None => {
                    let dependencies = HashSet::from_iter(target_operation.dependencies().clone());

                    // As we weren't in a bubble we can directly apply this action.
                    let result = GroupCrdt::apply_action(
                        y,
                        target_operation.id(),
                        target_operation.author(),
                        &dependencies,
                        &target_operation.payload().action,
                    )?;

                    y = match result {
                        StateChangeResult::Ok { state } => state,
                        StateChangeResult::Noop { error, .. } => {
                            return Err(GroupCrdtError::StateChangeError(
                                target_operation.id(),
                                error,
                            ));
                        }
                        StateChangeResult::Filtered { state } => state,
                    };
                    visited.drain();
                }
            }
        }

        // Sanity check: all bubbles should be visited completely.
        assert!(bubbles.is_empty(), "{:?}", bubbles);
        Ok(y)
    }
}

impl<ID, OP, C, RS, ORD, GS> GroupCrdtState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    GS: GroupStore<ID, OP, C, RS, ORD> + Debug + Clone,
{
    /// If the given operation is an action which removes or demotes a manager member, return the
    /// ID of the target member.
    fn removed_manager(&self, operation: &ORD::Operation) -> Option<ID> {
        let action = operation.payload().action;

        let removed_or_demoted_member = match action {
            GroupAction::Remove { member } => member,
            GroupAction::Demote { member, .. } => member,
            _ => return None,
        };

        // @TODO: either remove this step (and check for mutual removes on all remove/demote
        // operations) or re-build graph state beforehand to in order to correctly handle
        // certain edge-cases.
        let was_manager = self
            .transitive_members_at(&HashSet::from_iter(operation.dependencies()))
            .expect("get transitive members")
            .contains(&(removed_or_demoted_member.id(), Access::manage()));

        if was_manager {
            Some(removed_or_demoted_member.id())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::Access;
    use crate::group::GroupMember;
    use crate::group::crdt::tests::{
        add_member, assert_members, create_group, from_create, remove_member, sync,
    };
    use crate::test_utils::Network;
    use crate::traits::OperationId;

    impl OperationId for &str {}

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
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
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
            vec![(GroupMember::Individual(claire), Access::manage()),]
        );

        let bob_members = network.members(&bob, &group);
        assert_eq!(
            bob_members,
            vec![(GroupMember::Individual(claire), Access::manage()),]
        );

        let claire_members = network.members(&claire, &group);
        assert_eq!(
            claire_members,
            vec![(GroupMember::Individual(claire), Access::manage()),]
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
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        // Everyone processes the operation.
        network.process();

        // Alice demotes Bob.
        network.demote(alice, GroupMember::Individual(bob), group, Access::write());

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
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::write()),
                (GroupMember::Individual(claire), Access::manage()),
            ]
        );

        let bob_members = network.members(&bob, &group);
        assert_eq!(
            bob_members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::write()),
                (GroupMember::Individual(claire), Access::manage()),
            ]
        );

        let claire_members = network.members(&claire, &group);
        assert_eq!(
            claire_members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::write()),
                (GroupMember::Individual(claire), Access::manage()),
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
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        // Everyone processes the operation.
        network.process();

        // Alice demotes Bob.
        network.demote(alice, GroupMember::Individual(bob), group, Access::write());

        // Bob adds Dave concurrently.
        network.add(bob, GroupMember::Individual(dave), group, Access::read());

        // Everyone processes these operations.
        network.process();

        // The demote operation should have been applied.
        // The add operation should have been filtered.

        // We expect Alice (Manage), Bob (Write) and Claire (Manage) to be the only group members.
        let expected_members = vec![
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::write()),
            (GroupMember::Individual(claire), Access::manage()),
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
        // Tree structure:
        //       1
        //     /   \
        //    2     3
        //           \
        //            4

        let group_id = '1';
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';

        let mut rng = StdRng::from_os_rng();

        // Create initial group with Alice and Bob
        let (alice_group, op_create) = create_group(
            alice,
            group_id,
            vec![(alice, Access::manage()), (bob, Access::manage())],
            &mut rng,
        );

        let bob_group = from_create(bob, group_id, &op_create, &mut rng);
        let claire_group = from_create(claire, group_id, &op_create, &mut rng);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
            ],
        );

        // Alice removes Bob
        let (alice_group, op_remove_bob) = remove_member(alice_group, group_id, bob);

        assert_members(
            &alice_group,
            &[(GroupMember::Individual(alice), Access::manage())],
        );

        // Bob (in his own branch) adds Claire
        let (bob_group, op_add_claire) = add_member(bob_group, group_id, claire, Access::manage());
        let claire_group = sync(claire_group, &[op_add_claire.clone()]);

        assert_members(
            &bob_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        // Claire adds Dave
        let (claire_group, op_add_dave) = add_member(claire_group, group_id, dave, Access::read());
        let bob_group = sync(bob_group, &[op_add_dave.clone()]);

        assert_members(
            &bob_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
                (GroupMember::Individual(dave), Access::read()),
            ],
        );

        // Merge branches: everyone sees each other's operations
        let alice_group = sync(alice_group, &[op_add_claire.clone(), op_add_dave.clone()]);
        let bob_group = sync(bob_group, &[op_remove_bob.clone()]);
        let claire_group = sync(claire_group, &[op_remove_bob.clone()]);

        let expected_members = vec![(GroupMember::Individual(alice), Access::manage())];

        assert_members(&alice_group, &expected_members);
        assert_members(&bob_group, &expected_members);
        assert_members(&claire_group, &expected_members);
    }

    #[test]
    fn remove_readd_dependencies_filter() {
        //       1
        //     /   \
        //    2     4
        //    |     |
        //    3     |
        //     \   /
        //       5
        //
        // Node 1: create the group with Alice, Bob and Claire as managers
        // Node 2: Alice removes Bob
        // Node 3: Alice re-adds Bob
        // Node 4: Bob adds Dave
        // Node 5: Bob adds Eve
        //
        // Filtered: [4]
        // Final members: [Alice, Bob, Claire, Eve]

        let group_id = 'G';
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';
        let eve = 'E';

        let mut rng = StdRng::from_os_rng();

        // 1: Alice creates group with Alice, Bob, Claire
        let (alice_group, op_create) = create_group(
            alice,
            group_id,
            vec![
                (alice, Access::manage()),
                (bob, Access::manage()),
                (claire, Access::manage()),
            ],
            &mut rng,
        );

        let bob_group = from_create(bob, group_id, &op_create, &mut rng);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        // 2: Alice removes Bob
        let (alice_group, op_remove_bob) = remove_member(alice_group, group_id, bob);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        // 3: Alice re-adds Bob
        let (alice_group, op_readd_bob) = add_member(alice_group, group_id, bob, Access::manage());

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        // 4: Bob adds Dave
        let (bob_group, op_add_dave) = add_member(bob_group, group_id, dave, Access::read());

        assert_members(
            &bob_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
                (GroupMember::Individual(dave), Access::read()),
            ],
        );

        // Everyone processes the concurrent operations
        let alice_group = sync(alice_group, &[op_add_dave.clone()]);
        let bob_group = sync(bob_group, &[op_remove_bob.clone(), op_readd_bob.clone()]);

        // Bob adds Eve
        let (bob_group, op_add_eve) = add_member(bob_group, group_id, eve, Access::read());
        let alice_group = sync(alice_group, &[op_add_eve.clone()]);

        // Final assertions: All 4 members should be present
        let expected = vec![
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::manage()),
            (GroupMember::Individual(claire), Access::manage()),
            (GroupMember::Individual(eve), Access::read()),
        ];

        assert_members(&alice_group, &expected);
        assert_members(&bob_group, &expected);
    }

    #[test]
    fn two_bubbles() {
        // Graph structure:
        //
        //       1
        //     /   \
        //    2     3
        //     \   /
        //       4
        //       |
        //       5
        //     /   \
        //    6     8
        //    |
        //    7
        //
        // 1: Alice creates group with initial members Alice (admin) & Bob (admin)
        // 2: Alice removes Bob
        // 3: Bob adds Claire
        // 4: Alice adds Dave (admin)
        // 5: Dave adds Eve
        // 6: Alice adds Frank
        // 7: Frank adds Grace
        // 8: Dave removes Alice
        //
        // Filtered (due to concurrent removal): [3, 6, 7]

        let group_id = '0';
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';
        let eve = 'E';
        let frank = 'F';
        let grace = 'G';

        let mut rng = StdRng::from_os_rng();

        // 1: Create initial group with Alice and Bob
        let (alice_group, op_create) = create_group(
            alice,
            group_id,
            vec![(alice, Access::manage()), (bob, Access::manage())],
            &mut rng,
        );

        // Initialize all member groups from the create operation
        let bob_group = from_create(bob, group_id, &op_create, &mut rng);
        let dave_group = from_create(dave, group_id, &op_create, &mut rng);
        let frank_group = from_create(frank, group_id, &op_create, &mut rng);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
            ],
        );

        // 2: Alice removes Bob
        let (alice_group, op_remove_bob) = remove_member(alice_group, group_id, bob);

        // 3: Bob adds Claire (concurrent with 2)
        let (_bob_group, op_add_claire) = add_member(bob_group, group_id, claire, Access::read());

        // Alice processes Bob's operation
        let alice_group = sync(alice_group, &[op_add_claire.clone()]);

        // 4: Alice adds Dave (merges states 2 & 3)
        let (alice_group, op_add_dave) = add_member(alice_group, group_id, dave, Access::manage());

        // New member Dave syncs state
        let dave_group = sync(
            dave_group,
            &[
                op_remove_bob.clone(),
                op_add_claire.clone(),
                op_add_dave.clone(),
            ],
        );

        // 5: Dave adds Eve (depends on 4)
        let (dave_group, op_add_eve) = add_member(dave_group, group_id, eve, Access::read());

        let alice_group = sync(alice_group, &[op_add_eve.clone()]);

        // 6: Alice adds Frank (concurrent with 8)
        let (_alice_group, op_add_frank) =
            add_member(alice_group, group_id, frank, Access::manage());

        let frank_group = sync(
            frank_group,
            &[
                op_remove_bob.clone(),
                op_add_claire.clone(),
                op_add_dave.clone(),
                op_add_eve.clone(),
                op_add_frank.clone(),
            ],
        );

        // 7: Frank adds Grace (concurrent with 8)
        let (_, op_add_grace) = add_member(frank_group, group_id, grace, Access::read());

        // 8: Dave removes Alice (concurrently with 6 & 7)
        let (dave_group, _op_remove_alice) = remove_member(dave_group, group_id, alice);

        let dave_group = sync(dave_group, &[op_add_frank.clone(), op_add_grace.clone()]);

        let expected_members = vec![
            (GroupMember::Individual(dave), Access::manage()),
            (GroupMember::Individual(eve), Access::read()),
        ];

        let mut dave_members = dave_group.members();
        dave_members.sort();
        assert_eq!(expected_members, dave_members);
    }
}
