// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strong remove group resolver implementation.
use petgraph::graphmap::DiGraphMap;
use petgraph::visit::{IntoNodeIdentifiers, Topo};
use std::collections::{HashMap, HashSet};
use std::{fmt::Debug, marker::PhantomData};

use crate::graph::{concurrent_bubbles, split_bubble};
use crate::group::crdt::{GroupCrdtInnerError, apply_remove_unsafe};
use crate::group::{
    AuthorityGraphs, GroupAction, GroupControlMessage, GroupCrdtInnerState, GroupMember,
    apply_action,
};
use crate::traits::{Conditions, IdentityHandle, Operation, OperationId, Resolver};

/// An implementation of `Resolver` trait which follows strong remove ruleset.  
///
/// Concurrent operations are identified and processed, any which should be invalidated are added
/// to the operation filter and not applied to the group state. Once an operation has been
/// filtered, any operations which depended on any resulting state will not be applied to group
/// state either. Ruleset for Concurrent Operations
///
/// The following ruleset is applied when choosing which operations to "filter" when concurrent
/// operations are processed. It can be assumed that the behavior is equivalent for an admin
/// member being removed, or demoted from admin to a lower access level.
///
/// ## Strong Remove Concurrency Rules
///
/// ### Removals
///
/// If a removal has occurred, filter any concurrent operations by the removed member, as long as
/// it's 1) not a predecessor of the remove operation, and 2) not a mutual removal (see below).
///
/// ### Mutual removals
///
/// Mutual removes can occur directly (A removes B, B removes C) or in a cycle (A removes B, B
/// removes C, C removes A), cycles may even include delegations by removed members. The result of
/// all of these cases should be that all members which were part of the cycle should be removed
/// from the group.
///
/// ### Re-adding member concurrently
///
/// If Alice removes Charlie and Bob removes then adds Charlie concurrently, Charlie is removed
/// from the group and all their concurrent actions should be filtered.
///
/// ### Filtering of transitively dependent operations
///
/// When an operation is "explicitly" filtered it may cause dependent operations to become
/// invalid, these operations should not be applied to the group state.
#[derive(Clone, Debug, Default)]
pub struct StrongRemove<ID, OP, C, M> {
    _phantom: PhantomData<(ID, OP, C, M)>,
}

impl<ID, OP, C, M> Resolver<ID, OP, C, M> for StrongRemove<ID, OP, C, M>
where
    ID: IdentityHandle + Ord,
    OP: OperationId + Ord,
    C: Conditions,
    M: Clone + Operation<ID, OP, GroupControlMessage<ID, C>>,
{
    type State = GroupCrdtInnerState<ID, OP, C, M>;
    type Error = GroupCrdtInnerError<OP>;

    /// Identify if an operation should trigger a group state rebuild.
    fn rebuild_required(y: &Self::State, operation: &M) -> Result<bool, Self::Error> {
        let dependencies = operation.dependencies().into_iter().collect();
        Ok(y.heads() != dependencies)
    }

    /// Process the group operation graph, producing a new filter and re-building all state
    /// accordingly.
    fn process(mut y: Self::State) -> Result<Self::State, Self::Error> {
        // Start by clearing the existing filter and re-building all states.
        y.ignore = HashSet::default();
        y.mutual_removes = HashSet::default();

        // Get all bubbles of concurrency.
        //
        // A concurrency bubble is a set of operations from the group graph which share some
        // concurrency. Multiple bubbles can occur in the same graph.
        let bubbles = concurrent_bubbles(&y.graph);
        let mut processed_bubbles = Vec::new();

        let mut topo = Topo::new(&y.graph);
        while let Some(operation_id) = topo.next(&y.graph) {
            let bubble = bubbles.iter().find(|bubble| bubble.contains(&operation_id));

            let Some(bubble) = bubble else {
                y = StrongRemove::apply_operation(y, operation_id);
                continue;
            };

            if processed_bubbles.iter().any(|b| b == &bubble) {
                continue;
            };

            y = StrongRemove::process_bubble(y, bubble);
            processed_bubbles.push(bubble);
        }

        Ok(y)
    }
}

impl<ID, OP, C, M> StrongRemove<ID, OP, C, M>
where
    ID: IdentityHandle + Ord,
    OP: OperationId + Ord,
    C: Conditions,
    M: Clone + Operation<ID, OP, GroupControlMessage<ID, C>>,
{
    fn process_bubble(
        mut y: GroupCrdtInnerState<ID, OP, C, M>,
        bubble: &HashSet<OP>,
    ) -> GroupCrdtInnerState<ID, OP, C, M> {
        // Remove all non-bubble operations from the graph.
        let bubble_graph = {
            let non_bubble_operations: Vec<_> = y
                .graph
                .node_identifiers()
                .filter(|n| !bubble.contains(n))
                .collect();

            let mut bubble_graph = y.graph.clone();
            for node_id in non_bubble_operations {
                bubble_graph.remove_node(node_id);
            }
            bubble_graph
        };

        // Construct mutual remove authority graph and concurrent remove filter.
        let mut authority_graphs = Self::build_authority_graphs(&y.operations, &bubble_graph);
        y = Self::compute_filter(y, bubble, &mut authority_graphs);

        // Iterate over all operations in the bubble starting from the root and proceeding in
        // topological order.
        //
        // NOTE: The petgraph topological sorting algorithm is only deterministic when all graph
        // nodes are added in the same order. This is not the case for us, but that's ok as we
        // don't rely on a deterministic linearisation of operations for resolving state, only
        // partial-ordering is required.
        let mut topo = Topo::new(&bubble_graph);
        while let Some(operation_id) = topo.next(&bubble_graph) {
            y = Self::apply_operation(y, operation_id);
        }
        y
    }

    fn is_mutual_remove(operation: &M, authority_graphs: &mut AuthorityGraphs<ID, OP>) -> bool {
        let removed = removed_or_demoted_manager(operation);
        let added = added_or_promoted_manager(operation);
        if removed.is_none() && added.is_none() {
            return false;
        }

        let group_id = operation.payload().group_id();
        authority_graphs.is_cycle(&group_id, &operation.id())
    }

    fn is_readd(group_id: ID, removed: ID, operation: &M) -> bool {
        if group_id != operation.payload().group_id() {
            return false;
        }

        let GroupAction::Add { member: added, .. } = &operation.payload().action else {
            return false;
        };

        added.id() == removed
    }

    fn is_removed(group_id: ID, removed: ID, operation: &M) -> bool {
        if group_id != operation.payload().group_id() {
            return false;
        }

        operation.author() == removed
    }

    /// Construct an operation filter based on "strong remove" rules.
    fn compute_filter(
        mut y: GroupCrdtInnerState<ID, OP, C, M>,
        bubble: &HashSet<OP>,
        authority_graphs: &mut AuthorityGraphs<ID, OP>,
    ) -> GroupCrdtInnerState<ID, OP, C, M> {
        // All operations which should be filtered out due to concurrent actions.
        let mut filter = HashSet::new();

        // All operations containing a removal/demote or add/promote which are part of a mutual
        // remove cycle.
        let mut mutual_removes = HashSet::new();

        // Iterate of all operations in the bubble.
        for operation_id in bubble {
            let operation = y
                .operations
                .get(operation_id)
                .expect("all operations present in map");

            // If this is not a remove or demote operation no action is required.
            let Some(removed) = removed_or_demoted_manager(operation) else {
                continue;
            };

            // RULE: Detect mutual remove cycles.
            if Self::is_mutual_remove(operation, authority_graphs) {
                mutual_removes.insert(*operation_id);
            }

            // Extend the filter with all concurrent operations from the removed author.
            let group_id = operation.payload().group_id();
            let (mut concurrent, ..) = split_bubble(&y.graph, bubble, *operation_id);
            concurrent.retain(|id| {
                let concurrent_operation =
                    y.operations.get(id).expect("all operations present in map");

                // RULE: Concurrent re-adds not allowed.
                let is_readd = Self::is_readd(group_id, removed, concurrent_operation);
                // RULE: Ignore concurrent actions by removed member.
                let is_removed = Self::is_removed(group_id, removed, concurrent_operation);
                is_removed || is_readd
            });

            filter.extend(concurrent.iter());
        }

        y.ignore = filter;
        y.mutual_removes = mutual_removes;
        y
    }

    /// Build the authority graph.
    fn build_authority_graphs(
        operations: &HashMap<OP, M>,
        bubble_graph: &DiGraphMap<OP, ()>,
    ) -> AuthorityGraphs<ID, OP> {
        let mut authority_graphs = AuthorityGraphs::new(bubble_graph.clone());

        // Iterate over every operation in the bubble.
        for id in bubble_graph.nodes() {
            let operation = operations.get(&id).expect("all operations present in map");
            let author = operation.author();
            let group_id = operation.payload().group_id();

            // If this is a remove or demote of a manager then add it to the authority graph.
            if let Some(removed) = removed_or_demoted_manager(operation) {
                authority_graphs.add_removal(group_id, author, removed, id);
            };

            // If this is a add or promote of a manager then add it to the authority graph.
            if let Some(added) = added_or_promoted_manager(operation) {
                authority_graphs.add_delegation(group_id, author, added, id);
            };
        }

        authority_graphs
    }

    fn apply_operation(
        mut y: GroupCrdtInnerState<ID, OP, C, M>,
        operation_id: OP,
    ) -> GroupCrdtInnerState<ID, OP, C, M> {
        let operation = y
            .operations
            .get(&operation_id)
            .expect("all processed operations exist");

        let dependencies = HashSet::from_iter(operation.dependencies().clone());

        let mut groups_y = y
            .state_at(&dependencies)
            .expect("all state objects to exist");

        groups_y = if !y.mutual_removes.contains(&operation_id) {
            apply_action(
                groups_y,
                operation.payload().group_id(),
                operation.id(),
                operation.author(),
                &operation.payload().action,
                &y.ignore,
            )
            .state()
            .to_owned()
        } else {
            let Some(removed) = removed_or_demoted_manager(operation) else {
                unreachable!();
            };

            // @TODO: Currently only individual managers are supported. This code will need
            // changing if support for group managers is introduced.
            apply_remove_unsafe(
                groups_y,
                operation.payload().group_id(),
                GroupMember::Individual(removed),
            )
        };
        y.states.insert(operation.id(), groups_y);
        y
    }
}

fn removed_or_demoted_manager<ID, OP, C, M>(operation: &M) -> Option<ID>
where
    ID: IdentityHandle + Ord,
    OP: OperationId + Ord,
    C: Conditions,
    M: Clone + Operation<ID, OP, GroupControlMessage<ID, C>>,
{
    let action = operation.payload().action;
    if let GroupAction::Remove { member: removed } = action {
        return Some(removed.id());
    }

    if let GroupAction::Demote {
        member: demoted,
        access,
    } = action
        && !access.is_manage()
    {
        return Some(demoted.id());
    }

    None
}

fn added_or_promoted_manager<ID, OP, C, M>(operation: &M) -> Option<ID>
where
    ID: IdentityHandle + Ord,
    OP: OperationId + Ord,
    C: Conditions,
    M: Clone + Operation<ID, OP, GroupControlMessage<ID, C>>,
{
    let action = operation.payload().action;
    if let GroupAction::Add {
        member: added,
        access,
    } = &action
        && access.is_manage()
    {
        return Some(added.id());
    }

    if let GroupAction::Promote {
        member: promoted,
        access,
    } = action
        && access.is_manage()
    {
        return Some(promoted.id());
    }

    None
}

#[cfg(test)]
mod tests {
    use crate::Access;
    use crate::group::GroupMember;
    use crate::test_utils::no_ord::TestGroupState;
    use crate::test_utils::{
        MemberId, add_member, assert_members, create_group, demote_member, remove_member, sync,
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

        assert_eq!(y_i.inner.ignore.len(), 2);
        assert_eq!(y_i.inner.mutual_removes.len(), 2);
    }

    #[test]
    fn mutual_remove_cycles_detected() {
        //      0
        //    / | \
        //   1  2  3
        //
        // 0: Alice creates a group with Alice, Bob, Claire
        // 1: Alice removes Bob
        // 2: Bob removes Claire
        // 3: Claire removes Alice
        //
        // All three removals form a cycle
        // Final members: []

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

        // 2: Bob removes Claire
        let op2 = remove_member(BOB, 2, G1, GroupMember::Individual(CLAIRE), vec![op0.id()]);

        // 3: Claire removes Alice
        let op3 = remove_member(
            CLAIRE,
            3,
            G1,
            GroupMember::Individual(ALICE),
            vec![op0.id()],
        );

        let expected: Vec<(MemberId, Access)> = vec![];

        let y_i = sync(y, &[op0, op1, op2, op3]);
        assert_members(&y_i, G1, &expected);

        assert_eq!(y_i.inner.ignore.len(), 3);
        assert_eq!(y_i.inner.mutual_removes.len(), 3);
    }

    #[test]
    fn mutual_remove_cycle_with_delegation() {
        //      0
        //    / | \
        //   1  2  3
        //         |
        //         4
        //
        // 0: Alice creates a group with Alice, Bob, Claire
        // 1: Alice removes Bob
        // 2: Bob removes Claire
        // 3: Claire adds Dave
        // 4: Dave removes Alice
        //
        // The removals and add forms a transitive cycle
        // Final members: []

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

        // 2: Bob removes Claire
        let op2 = remove_member(BOB, 2, G1, GroupMember::Individual(CLAIRE), vec![op0.id()]);

        // 3: Claire adds Dave
        let op3 = add_member(
            CLAIRE,
            3,
            G1,
            GroupMember::Individual(DAVE),
            Access::manage(),
            vec![op0.id()],
        );

        // 4: Dave removes Alice
        let op4 = remove_member(DAVE, 4, G1, GroupMember::Individual(ALICE), vec![op3.id()]);

        let expected: Vec<(MemberId, Access)> = vec![]; // No members left

        let y_i = sync(y, &[op0, op1, op2, op3, op4]);
        assert_members(&y_i, G1, &expected);
        assert_eq!(y_i.inner.ignore.len(), 3);
        assert_eq!(y_i.inner.mutual_removes.len(), 3);
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

    #[test]
    fn concurrent_readds_filtered() {
        //       0
        //     /   \
        //    1     3
        //    |
        //    2
        //
        // 0: Alice creates group with Alice, Bob, Claire
        // 1: Alice removes Bob
        // 2: Alice re-adds Bob
        // 3: Claire removes Bob
        //
        // Filtered: [2]
        // Final members: [Alice, Claire]

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

        // 3: Claire removes Bob
        let op3 = remove_member(CLAIRE, 3, G1, GroupMember::Individual(BOB), vec![op0.id()]);

        let expected = vec![(ALICE, Access::manage()), (CLAIRE, Access::manage())];

        let y_final = sync(y, &[op0, op1, op2, op3]);
        assert_members(&y_final, G1, &expected);
    }

    #[test]
    fn filter_only_concurrent_operations() {
        let y = TestGroupState::new(());

        // 0: Alice creates a group
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

        // 1: Bob adds Claire
        let op1 = add_member(
            BOB,
            1,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::read(),
            vec![op0.id()],
        );

        // 2: Alice concurrently demotes Bob
        let op2 = demote_member(
            ALICE,
            2,
            G1,
            GroupMember::Individual(BOB),
            Access::write(),
            vec![op1.id()],
        );

        // 3: Bob concurrently adds Dave
        let op3 = add_member(
            BOB,
            3,
            G1,
            GroupMember::Individual(DAVE),
            Access::read(),
            vec![op1.id()],
        );

        let expected = vec![
            (ALICE, Access::manage()),
            (BOB, Access::write()),
            (CLAIRE, Access::read()),
        ];
        let y_i = sync(y, &[op0, op1, op2, op3]);
        assert_members(&y_i, G1, &expected);
    }
}
