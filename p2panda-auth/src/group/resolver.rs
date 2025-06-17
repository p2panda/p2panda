// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group state resolver implementation.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::visit::DfsPostOrder;
use thiserror::Error;

use crate::group::graph::{concurrent_bubbles, has_path};
use crate::group::{Access, Group, GroupControlMessage, GroupMember, GroupState, StateChangeResult};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

use super::GroupAction;

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
    GS: GroupStore<ID, OP, C, Self, ORD> + Debug + Clone,
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

        // We only need to rebuild the graph if this operation is concurrent.
        Ok(y.heads().into_iter().collect::<Vec<_>>() != operation.previous())
    }

    /// Resolve group membership by processing all concurrent operations in the graph.
    ///
    /// The following ruleset is applied when choosing which operations to "filter":
    ///
    /// 1) Mutual removals
    ///
    /// Mutual removals result in both members being removed from the group. Future implementations might
    /// take different approaches such as resolving by seniority, hash id, quorum or some other parameter.
    ///
    /// If a mutual removal occurs, the removal operations are retained but all concurrent
    /// operations performed by the removed members are filtered.
    ///
    /// 2) Re-adding member concurrently
    ///
    /// Concurrent re-adds are accepted. If Alice removes Charlie and Bob removes then adds Charlie
    /// concurrently, Charlie is still in the group.
    ///
    /// 3) Removed admin performing concurrent actions
    ///
    /// If Alice removes Bob, all of Bob's concurrent actions are filtered.
    ///
    /// 4) Demoted admin performing concurrent actions
    ///
    /// If Alice demotes Bob (from admin), Bob is no longer an admin and all of his concurrent
    /// actions are filtered.
    fn process(
        mut y: GroupState<ID, OP, C, Self, ORD, GS>,
    ) -> Result<GroupState<ID, OP, C, Self, ORD, GS>, Self::Error> {
        // Start by draining the existing filter and re-building all states.
        y.ignore.drain();
        let mut y = Group::rebuild(y).expect("no errors when re-building a group");

        let mut filter: HashSet<OP> = Default::default();
        let operations: HashMap<OP, ORD::Message> = y
            .operations
            .clone()
            .into_iter()
            .map(|op| (op.id(), op))
            .collect();

        let mut mutual_removes = HashSet::new();

        let mut bubbles = concurrent_bubbles(&y.graph);

        let root = y.root();
        let mut dfs = DfsPostOrder::new(&y.graph, root);
        let mut visited = HashSet::new();
        while let Some(target_operation_id) = dfs.next(&y.graph) {
            let Some(target_operation) = operations.get(&target_operation_id) else {
                return Err(GroupResolverError::MissingOperation(target_operation_id));
            };

            let bubble = bubbles
                .iter()
                .find(|bubble| bubble.contains(&target_operation_id))
                .cloned();

            visited.insert(target_operation_id);

            // Does the target operation remove or demote a manager member?
            let removed_manager = y.removed_manager(target_operation);

            if let (Some(removed_manager), Some(bubble)) = (removed_manager, &bubble) {
                for bubble_operation_id in bubble.iter() {
                    // If there's a path between the bubble and target operation, then it's not
                    // concurrent, so we don't need to do anything.
                    if has_path(&y.graph, *bubble_operation_id, target_operation_id) {
                        continue;
                    }

                    let Some(bubble_operation) = operations.get(bubble_operation_id) else {
                        return Err(GroupResolverError::MissingOperation(*bubble_operation_id));
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
                        // The removed member is concurrently removing the remover.
                        if concurrent_removed_admin == target_operation.author() {
                            // We don't want to filter out mutual remove/demote operations, but we
                            // still want to filter any dependent operations for both (mutually)
                            // removed members.
                            //
                            // The "target" operations are included when collecting invalid dependent
                            // operations, we record mutual remove operations here and then remove
                            // them from the filter later.
                            mutual_removes.insert(*bubble_operation_id);
                        }
                    }
                }
            }

            match bubble {
                Some(bubble) => {
                    if bubble.is_subset(&visited) {
                        let mut filter_tmp = filter.clone();
                        filter_tmp.retain(|op: &OP| !mutual_removes.contains(op));
                        y.ignore = filter_tmp;
                        y = Group::rebuild(y).expect("no errors when re-building a group");

                        // Remove the visited bubble.
                        bubbles.retain(|b| *b != bubble);

                        // Drain visited.
                        visited.drain();
                    }
                }
                None => {
                    y = match target_operation.payload() {
                        GroupControlMessage::GroupAction { action, .. } => {
                            let previous_operations =
                                HashSet::from_iter(target_operation.previous().clone());
                            match Group::apply_action(
                                y,
                                target_operation.id(),
                                GroupMember::Individual(target_operation.author()),
                                &previous_operations,
                                &action,
                            ) {
                                StateChangeResult::Ok { state } => state,
                                StateChangeResult::Noop { state, .. } => state,
                                StateChangeResult::Filtered { state } => state,
                            }
                        }
                        GroupControlMessage::Revoke { .. } => unimplemented!(),
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

impl<ID, OP, C, RS, ORD, GS> GroupState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ORD::Message> + Debug,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Debug,
    GS: GroupStore<ID, OP, C, RS, ORD> + Debug + Clone,
{
    /// If the given operation is an action which removes a member or demotes a manager, return the
    /// ID of the target member.
    fn removed_manager(&self, operation: &ORD::Message) -> Option<ID> {
        let GroupControlMessage::GroupAction { action, .. } = operation.payload() else {
            // Revoke operations not yet supported.
            unimplemented!()
        };

        let removed_or_demoted_member = match action {
            GroupAction::Remove { member } => member,
            GroupAction::Demote { member, .. } => member,
            _ => return None,
        };

        // We only need to react to a filtered demote operation if the target author did have
        // admin access but now doesn't.
        let was_manager = self
            .transitive_members_at(&HashSet::from_iter(operation.dependencies()))
            .expect("state exists for all operations")
            .contains(&(removed_or_demoted_member.id(), Access::Manage));

        if was_manager {
            Some(removed_or_demoted_member.id())
        } else {
            None
        }
    }

    fn root(&self) -> OP {
        self.operations
            .iter()
            .find(|operation| {
                let GroupControlMessage::GroupAction { action, .. } = operation.payload() else {
                    // Revoke operations not yet supported.
                    unimplemented!()
                };

                matches!(action, GroupAction::Create { .. })
            })
            .expect("at least one create operation")
            .id()
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::group::test_utils::{
        Network, TestGroup, TestGroupState, TestGroupStore, TestOperation, TestOrdererState,
    };
    use crate::group::{Access, GroupAction, GroupControlMessage, GroupMember};
    use crate::traits::{AuthGroup, OperationId};

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

    fn from_create(
        group_id: char,
        actor_id: char,
        op_create: &TestOperation,
        rng: &mut StdRng,
    ) -> TestGroupState {
        let store = TestGroupStore::default();
        let orderer = TestOrdererState::new(actor_id, store.clone(), StdRng::from_rng(rng));
        let group = TestGroupState::new(group_id, actor_id, store, orderer);
        TestGroup::process(group, op_create).unwrap()
    }

    fn create_group(
        group_id: char,
        actor_id: char,
        members: Vec<(char, Access<()>)>,
        rng: &mut StdRng,
    ) -> (TestGroupState, TestOperation) {
        let store = TestGroupStore::default();
        let orderer = TestOrdererState::new(actor_id, store.clone(), StdRng::from_rng(rng));
        let group = TestGroupState::new(group_id, actor_id, store, orderer);
        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Create {
                initial_members: members
                    .into_iter()
                    .map(|(id, access)| (GroupMember::Individual(id), access))
                    .collect(),
            },
        };
        let (group, op) = TestGroup::prepare(group, &control_message).unwrap();
        let group = TestGroup::process(group, &op).unwrap();
        (group, op)
    }

    fn add_member(
        group: TestGroupState,
        group_id: char,
        member: char,
        access: Access<()>,
    ) -> (TestGroupState, TestOperation) {
        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(member),
                access,
            },
        };
        let (group, op) = TestGroup::prepare(group, &control_message).unwrap();
        let group = TestGroup::process(group, &op).unwrap();
        (group, op)
    }

    fn remove_member(
        group: TestGroupState,
        group_id: char,
        member: char,
    ) -> (TestGroupState, TestOperation) {
        let control_message = GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(member),
            },
        };
        let (group, op) = TestGroup::prepare(group, &control_message).unwrap();
        let group = TestGroup::process(group, &op).unwrap();
        (group, op)
    }

    fn sync(group: TestGroupState, ops: &[TestOperation]) -> TestGroupState {
        ops.iter()
            .fold(group, |g, op| TestGroup::process(g, op).unwrap())
    }

    fn assert_members(group: &TestGroupState, expected: &[(GroupMember<char>, Access<()>)]) {
        let mut actual = group.members();
        let mut expected = expected.to_vec();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
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
            group_id,
            alice,
            vec![(alice, Access::Manage), (bob, Access::Manage)],
            &mut rng,
        );

        let bob_group = from_create(group_id, bob, &op_create, &mut rng);
        let claire_group = from_create(group_id, claire, &op_create, &mut rng);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
            ],
        );

        // Alice removes Bob
        let (alice_group, op_remove_bob) = remove_member(alice_group, group_id, bob);

        assert_members(
            &alice_group,
            &[(GroupMember::Individual(alice), Access::Manage)],
        );

        // Bob (in his own branch) adds Claire
        let (bob_group, op_add_claire) = add_member(bob_group, group_id, claire, Access::Manage);
        let claire_group = sync(claire_group, &[op_add_claire.clone()]);

        assert_members(
            &bob_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // Claire adds Dave
        let (claire_group, op_add_dave) = add_member(claire_group, group_id, dave, Access::Read);
        let bob_group = sync(bob_group, &[op_add_dave.clone()]);

        assert_members(
            &bob_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
                (GroupMember::Individual(dave), Access::Read),
            ],
        );

        // Merge branches: everyone sees each other's operations
        let alice_group = sync(alice_group, &[op_add_claire.clone(), op_add_dave.clone()]);
        let bob_group = sync(bob_group, &[op_remove_bob.clone()]);
        let claire_group = sync(claire_group, &[op_remove_bob.clone()]);

        let expected_members = vec![(GroupMember::Individual(alice), Access::Manage)];

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
            group_id,
            alice,
            vec![
                (alice, Access::Manage),
                (bob, Access::Manage),
                (claire, Access::Manage),
            ],
            &mut rng,
        );

        let bob_group = from_create(group_id, bob, &op_create, &mut rng);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // 2: Alice removes Bob
        let (alice_group, op_remove_bob) = remove_member(alice_group, group_id, bob);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // 3: Alice re-adds Bob
        let (alice_group, op_readd_bob) = add_member(alice_group, group_id, bob, Access::Manage);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
            ],
        );

        // 4: Bob adds Dave
        let (bob_group, op_add_dave) = add_member(bob_group, group_id, dave, Access::Read);

        assert_members(
            &bob_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
                (GroupMember::Individual(claire), Access::Manage),
                (GroupMember::Individual(dave), Access::Read),
            ],
        );

        // Everyone processes the concurrent operations
        let alice_group = sync(alice_group, &[op_add_dave.clone()]);
        let bob_group = sync(bob_group, &[op_remove_bob.clone(), op_readd_bob.clone()]);

        // Bob adds Eve
        let (bob_group, op_add_eve) = add_member(bob_group, group_id, eve, Access::Read);
        let alice_group = sync(alice_group, &[op_add_eve.clone()]);

        // Final assertions: All 4 members should be present
        let expected = vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Manage),
            (GroupMember::Individual(claire), Access::Manage),
            (GroupMember::Individual(eve), Access::Read),
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
            group_id,
            alice,
            vec![(alice, Access::Manage), (bob, Access::Manage)],
            &mut rng,
        );

        // Initialize all member groups from the create operation
        let bob_group = from_create(group_id, bob, &op_create, &mut rng);
        let dave_group = from_create(group_id, dave, &op_create, &mut rng);
        let frank_group = from_create(group_id, frank, &op_create, &mut rng);

        assert_members(
            &alice_group,
            &[
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(bob), Access::Manage),
            ],
        );

        // 2: Alice removes Bob
        let (alice_group, op_remove_bob) = remove_member(alice_group, group_id, bob);

        // 3: Bob adds Claire (concurrent with 2)
        let (_bob_group, op_add_claire) = add_member(bob_group, group_id, claire, Access::Read);

        // Alice processes Bob's operation
        let alice_group = sync(alice_group, &[op_add_claire.clone()]);

        // 4: Alice adds Dave (merges states 2 & 3)
        let (alice_group, op_add_dave) = add_member(alice_group, group_id, dave, Access::Manage);

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
        let (dave_group, op_add_eve) = add_member(dave_group, group_id, eve, Access::Read);

        let alice_group = sync(alice_group, &[op_add_eve.clone()]);

        // 6: Alice adds Frank (concurrent with 8)
        let (_alice_group, op_add_frank) = add_member(alice_group, group_id, frank, Access::Pull);

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
        let (_, op_add_grace) = add_member(frank_group, group_id, grace, Access::Read);

        // 8: Dave removes Alice (concurrently with 6 & 7)
        let (dave_group, _op_remove_alice) = remove_member(dave_group, group_id, alice);

        let dave_group = sync(dave_group, &[op_add_frank.clone(), op_add_grace.clone()]);

        let expected_members = vec![
            (GroupMember::Individual(dave), Access::Manage),
            (GroupMember::Individual(eve), Access::Read),
        ];

        let mut dave_members = dave_group.members();
        dave_members.sort();
        assert_eq!(expected_members, dave_members);
    }
}
