// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strong remove group resolver implementation.
use std::collections::{HashMap, HashSet};
use std::{fmt::Debug, marker::PhantomData};

use petgraph::algo::toposort;
use thiserror::Error;

use crate::graph::{concurrent_bubbles, has_path};
use crate::group::crdt::GroupCrdtInnerError;
use crate::group::{GroupCrdtInnerState, GroupAction, GroupControlMessage, apply_action};
use crate::traits::{Conditions, IdentityHandle, Operation, OperationId, Resolver};

/// Error types for GroupCrdt.
#[derive(Debug, Error)]
pub enum StrongRemoveError<OP>
where
    OP: OperationId,
{
    #[error(transparent)]
    Groups(#[from] GroupCrdtInnerError<OP>),
}

#[derive(Clone, Debug, Default)]
pub struct StrongRemove<ID, OP, C, M> {
    _phantom: PhantomData<(ID, OP, C, M)>,
}

impl<ID, OP, C, M> Resolver<ID, OP, C, M> for StrongRemove<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    M: Clone + Operation<ID, OP, GroupControlMessage<ID, C>>,
{
    type State = GroupCrdtInnerState<ID, OP, C, M>;
    type Error = StrongRemoveError<OP>;

    /// Identify if an operation should trigger a group state rebuild.
    fn rebuild_required(y: &Self::State, operation: &M) -> Result<bool, Self::Error> {
        let dependencies = operation.dependencies().into_iter().collect();
        Ok(y.heads() != dependencies)
    }

    /// Process the group operation graph, producing a new filter and re-building all state
    /// accordingly.
    fn process(mut y: Self::State) -> Result<Self::State, Self::Error> {
        // Start by draining the existing filter and re-building all states.
        y.ignore.drain();
        let mut filter: HashSet<OP> = Default::default();

        let operations: HashMap<OP, M> = y.operations.drain().collect();

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
        let mut visited = Vec::new();

        // Traverse the graph visiting the operations in topological order.
        for target_operation_id in topo_sort.iter() {
            let target_operation = operations.get(target_operation_id).expect("all processed operations exist");
            let bubble = bubbles
                .iter()
                .find(|bubble| bubble.contains(target_operation_id))
                .cloned();

            visited.push(*target_operation_id);

            let control_message = target_operation.payload();
            let removed_or_demoted_member = match control_message.action {
                GroupAction::Remove { member } | GroupAction::Demote { member, .. } => {
                    Some(member.id())
                }
                _ => None,
            };

            // If this operation removes/demotes a member with manager rights _and_ it is part of
            // a concurrency bubble then we need to check the bubble for any operations which
            // should be filtered.
            if let (Some(removed_manager), Some(bubble)) =
                (removed_or_demoted_member, bubble.as_ref())
            {
                for bubble_operation_id in bubble.iter() {
                    // If there's a path between the bubble and target operation, then it's not
                    // concurrent, so we don't need to do anything.
                    if has_path(&y.graph, *bubble_operation_id, *target_operation_id) {
                        continue;
                    }

                    let bubble_operation = operations.get(bubble_operation_id).expect("all processed operations exist");

                    // The bubble operation is for a different group so it should not be filtered.
                    if bubble_operation.payload().group_id()
                        != target_operation.payload().group_id()
                    {
                        continue;
                    }

                    // If this concurrent operation is _not_ authored by the "target author" then we
                    // can continue to the next concurrent operation without taking any action.
                    if bubble_operation.author() != removed_manager {
                        continue;
                    }

                    // Add the concurrent operation to our filter.
                    filter.insert(*bubble_operation_id);

                    // Check if this was a concurrent remove.
                    let mutual_remove = match bubble_operation.payload().action {
                        GroupAction::Remove { member } | GroupAction::Demote { member, .. } => {
                            member.id() == target_operation.author()
                        }
                        _ => false,
                    };

                    if mutual_remove {
                        mutual_removes.insert(*bubble_operation_id);
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
                    if bubble.is_subset(&visited.iter().cloned().collect()) {
                        let mut filter_tmp = filter.clone();
                        filter_tmp.retain(|op: &OP| !mutual_removes.contains(op));
                        y.ignore = filter_tmp;
                        for op in visited {
                            let operation = operations.get(&op).expect("all operations exist");
                            let dependencies = HashSet::from_iter(operation.dependencies().clone());

                            let mut groups_y = y.state_at(&dependencies)?;

                            let result = apply_action(
                                groups_y,
                                operation.payload().group_id(),
                                operation.id(),
                                operation.author(),
                                &operation.payload().action,
                                &y.ignore,
                            );

                            groups_y = result.state().clone();
                            y.states.insert(operation.id(), groups_y);
                        }
                        visited = vec![];
                        bubbles.retain(|b| *b != bubble);
                    }
                }
                None => {
                    let dependencies = HashSet::from_iter(target_operation.dependencies().clone());

                    // As we weren't in a bubble we can directly apply this action.
                    let mut groups_y = y.state_at(&dependencies)?;
                    let result = apply_action(
                        groups_y,
                        target_operation.payload().group_id(),
                        target_operation.id(),
                        target_operation.author(),
                        &target_operation.payload().action,
                        &y.ignore,
                    );

                    groups_y = result.state().clone();
                    y.states.insert(target_operation.id(), groups_y);

                    visited = vec![];
                }
            }
        }

        // Sanity check: all bubbles should be visited completely.
        assert!(bubbles.is_empty(), "{bubbles:?}");

        y.operations = operations;

        Ok(y)
    }
}

#[cfg(test)]
mod tests {
    use crate::Access;
    use crate::group::GroupMember;
    use crate::test_utils::no_ord::TestGroupState;
    use crate::test_utils::{
        add_member, assert_members, create_group, demote_member, remove_member, sync,
    };

    use super::*;

    const G0: char = '0';
    const G1: char = '1';

    const ALICE: char = 'A';
    const BOB: char = 'B';
    const CLAIRE: char = 'C';
    const DAVE: char = 'D';
    const EVE: char = 'E';
    const FRANK: char = 'F';
    const GRACE: char = 'G';

    #[test]
    fn mutual_removal_filter() {
        //       0
        //     /   \
        //    1     2
        //
        // 0: Alice creates a group (Alice, Bob, Claire manage)
        // 1: Alice removes Bob
        // 2: Bob removes Alice
        //
        // Both removals should be processed, Claire remains

        let y = TestGroupState::new(());

        // 0: Alice creates a group
        let op0 = create_group(
            ALICE,
            0,
            G1,
            vec![
                (GroupMember::Individual(ALICE), Access::manage()),
                (GroupMember::Individual(BOB), Access::manage()),
                (GroupMember::Individual(CLAIRE), Access::manage()),
            ],
            vec![],
        );

        // 1: Alice removes Bob
        let op1 = remove_member(ALICE, 1, G1, GroupMember::Individual(BOB), vec![op0.id()]);

        // 2: Bob removes Alice
        let op2 = remove_member(BOB, 2, G1, GroupMember::Individual(ALICE), vec![op0.id()]);

        let expected = vec![(CLAIRE, Access::manage())];
        let y_i = sync(y, &[op0, op1, op2]);
        assert_members(&y_i, G1, &expected);

        // Ignore set should be empty
        assert!(y_i.auth_y.ignore.is_empty());
    }

    #[test]
    fn demote_remove_filter() {
        //       0
        //     /   \
        //    1     2
        //
        // 0: Alice creates a group with Alice, Bob, Claire (manage)
        // 1: Alice demotes Bob to Write
        // 2: Bob removes Claire (should be filtered)
        //
        // Final members: [Alice (manage), Bob (write), Claire (manage)]

        let y = TestGroupState::new(());

        // 0: Alice creates a group
        let op0 = create_group(
            ALICE,
            0,
            G1,
            vec![
                (GroupMember::Individual(ALICE), Access::manage()),
                (GroupMember::Individual(BOB), Access::manage()),
                (GroupMember::Individual(CLAIRE), Access::manage()),
            ],
            vec![],
        );

        // 1: Alice demotes Bob to Write
        let op1 = demote_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::write(),
            vec![op0.id()],
        );

        // 2: Bob removes Claire concurrently (should be filtered)
        let op2 = remove_member(BOB, 2, G1, GroupMember::Individual(CLAIRE), vec![op0.id()]);

        let expected = vec![
            (ALICE, Access::manage()),
            (BOB, Access::write()),
            (CLAIRE, Access::manage()),
        ];

        let y_final = sync(y, &[op0, op1, op2]);
        assert_members(&y_final, G1, &expected);
    }

    #[test]
    fn demote_add_filter() {
        //       0
        //     /   \
        //    1     2
        //
        // 0: Alice creates a group with Alice, Bob, Claire (manage)
        // 1: Alice demotes Bob to Write
        // 2: Bob adds Dave (should be filtered)
        //
        // Final members: [Alice (manage), Bob (write), Claire (manage)]

        let y = TestGroupState::new(());

        // 0: Alice creates a group
        let op0 = create_group(
            ALICE,
            0,
            G1,
            vec![
                (GroupMember::Individual(ALICE), Access::manage()),
                (GroupMember::Individual(BOB), Access::manage()),
                (GroupMember::Individual(CLAIRE), Access::manage()),
            ],
            vec![],
        );

        // 1: Alice demotes Bob to Write
        let op1 = demote_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::write(),
            vec![op0.id()],
        );

        // 2: Bob adds Dave concurrently (should be filtered)
        let op2 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(DAVE),
            Access::read(),
            vec![op0.id()],
        );

        let expected = vec![
            (ALICE, Access::manage()),
            (BOB, Access::write()),
            (CLAIRE, Access::manage()),
        ];

        let y_i = sync(y, &[op0, op1, op2]);
        assert_members(&y_i, G1, &expected);
    }

    #[test]
    fn remove_dependencies_filter() {
        // Tree structure:
        //       0
        //     /   \
        //    1     2
        //           \
        //            3
        //
        // 0: Alice creates group with Alice and Bob (manage)
        // 1: Alice removes Bob
        // 2: Bob adds Claire (concurrent with 1, Bob's branch)
        // 3: Claire adds Dave (depends on 2, Claire's branch)
        //
        // After merging, only Alice remains (removals win).

        let y = TestGroupState::new(());

        // 0: Create initial group with Alice and Bob
        let op0 = create_group(
            ALICE,
            0,
            G1,
            vec![
                (GroupMember::Individual(ALICE), Access::manage()),
                (GroupMember::Individual(BOB), Access::manage()),
            ],
            vec![],
        );

        // 1: Alice removes Bob
        let op1 = remove_member(ALICE, 1, G1, GroupMember::Individual(BOB), vec![op0.id()]);

        // 2: Bob adds Claire (concurrent with 1)
        let op2 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::manage(),
            vec![op0.id()],
        );

        // 3: Claire adds Dave (in Bob's branch)
        let op3 = add_member(
            CLAIRE,
            3,
            G1,
            GroupMember::Individual(DAVE),
            Access::read(),
            vec![op2.id()],
        );

        // Only assert final state
        let expected_members = vec![(ALICE, Access::manage())];
        let y_i = sync(y, &[op0, op1, op2, op3]);
        assert_members(&y_i, G1, &expected_members);
    }
    #[test]
    fn remove_readd_dependencies_filter() {
        //       0
        //     /   \
        //    1     3
        //    |     |
        //    2     |
        //     \   /
        //       4
        //
        // 0: Alice creates group with Alice, Bob, Claire (manage)
        // 1: Alice removes Bob
        // 2: Alice re-adds Bob
        // 3: Bob adds Dave (concurrent with re-add)
        // 4: Bob adds Eve (after re-add and Dave in Bob's branch)
        //
        // Filtered: [3]
        // Final members: [Alice, Bob, Claire, Eve]

        let y = TestGroupState::new(());

        // 0: Alice creates group with Alice, Bob, Claire
        let op0 = create_group(
            ALICE,
            0,
            G1,
            vec![
                (GroupMember::Individual(ALICE), Access::manage()),
                (GroupMember::Individual(BOB), Access::manage()),
                (GroupMember::Individual(CLAIRE), Access::manage()),
            ],
            vec![],
        );

        // 1: Alice removes Bob
        let op1 = remove_member(ALICE, 1, G1, GroupMember::Individual(BOB), vec![op0.id()]);

        // 2: Alice re-adds Bob
        let op2 = add_member(
            ALICE,
            2,
            G1,
            GroupMember::Individual(BOB),
            Access::manage(),
            vec![op1.id()],
        );

        // 3: Bob adds Dave (concurrent with 2)
        let op3 = add_member(
            BOB,
            3,
            G1,
            GroupMember::Individual(DAVE),
            Access::read(),
            vec![op0.id()],
        );

        // 4: Bob adds Eve (after Bob re-added, depends on Bob's add Dave)
        let op4 = add_member(
            BOB,
            4,
            G1,
            GroupMember::Individual(EVE),
            Access::read(),
            vec![op2.id(), op3.id()],
        );

        let expected = vec![
            (ALICE, Access::manage()),
            (BOB, Access::manage()),
            (CLAIRE, Access::manage()),
            (EVE, Access::read()),
        ];

        let y_final = sync(y, &[op0, op1, op2, op3, op4]);
        assert_members(&y_final, G1, &expected);
    }

    #[test]
    fn two_bubbles() {
        // Graph structure:
        //
        //       0
        //     /   \
        //    1     2
        //     \   /
        //       3
        //       |
        //       4
        //     /   \
        //    5     7
        //    |
        //    6
        //
        // 0: Alice creates group with initial members Alice (manage) & Bob (manage)
        // 1: Alice removes Bob
        // 2: Bob adds Claire
        // 3: Alice adds Dave (manage)
        // 4: Dave adds Eve
        // 5: Alice adds Frank
        // 6: Frank adds Grace
        // 7: Dave removes Alice

        let y = TestGroupState::new(());

        // 0: Create initial group with Alice and Bob
        let op0 = create_group(
            ALICE,
            0,
            G0,
            vec![
                (GroupMember::Individual(ALICE), Access::manage()),
                (GroupMember::Individual(BOB), Access::manage()),
            ],
            vec![],
        );

        // 1: Alice removes Bob
        let op1 = remove_member(ALICE, 1, G0, GroupMember::Individual(BOB), vec![op0.id()]);

        // 2: Bob adds Claire (concurrent with 1)
        let op2 = add_member(
            BOB,
            2,
            G0,
            GroupMember::Individual(CLAIRE),
            Access::read(),
            vec![op0.id()],
        );

        // 3: Alice adds Dave (merges states 1 & 2)
        let op3 = add_member(
            ALICE,
            3,
            G0,
            GroupMember::Individual(DAVE),
            Access::manage(),
            vec![op1.id(), op2.id()],
        );

        // 4: Dave adds Eve
        let op4 = add_member(
            DAVE,
            4,
            G0,
            GroupMember::Individual(EVE),
            Access::read(),
            vec![op3.id()],
        );

        // 5: Alice adds Frank
        let op5 = add_member(
            ALICE,
            5,
            G0,
            GroupMember::Individual(FRANK),
            Access::manage(),
            vec![op4.id()],
        );

        // 6: Frank adds Grace
        let op6 = add_member(
            FRANK,
            6,
            G0,
            GroupMember::Individual(GRACE),
            Access::read(),
            vec![op5.id()],
        );

        // 6: Dave removes alice concurrently
        let op7 = remove_member(DAVE, 7, G0, GroupMember::Individual(ALICE), vec![op4.id()]);

        let expected_members = vec![(DAVE, Access::manage()), (EVE, Access::read())];
        let y_i = sync(y, &[op0, op1, op2, op3, op4, op5, op6, op7]);
        assert_members(&y_i, G0, &expected_members);
    }
}
