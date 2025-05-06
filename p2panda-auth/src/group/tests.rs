use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::group::test_utils::{TestOrderer, TestOrdererState};
use crate::group::{Group, GroupState};
use crate::traits::{AuthGraph, IdentityHandle, OperationId};

use super::access::Access;
use super::resolver::GroupResolver;
use super::test_utils::{MemoryStore as GroupMemoryStore, TestOperation};
use super::{GroupAction, GroupControlMessage, GroupMember};

impl IdentityHandle for char {}
impl OperationId for u32 {}

type TestMemoryStore = GroupMemoryStore<char, u32, TestOperation<char, u32>>;
type TestResolver = GroupResolver<char, u32, TestOperation<char, u32>>;
type TestGroup = Group<char, u32, TestResolver, TestOrderer, TestMemoryStore>;

#[test]
fn basic_group() {
    let alice = 'A';
    let orderer_y = TestOrdererState {
        my_id: alice,
        operations: Default::default(),
    };
    let group_store = TestMemoryStore::default();
    let group_y = GroupState::new(alice, alice, group_store, orderer_y);

    // Create group with alice as initial admin member.
    let control_message_001 = GroupControlMessage::GroupAction {
        action: GroupAction::Create {
            initial_members: vec![(GroupMember::Individual(alice), Access::Manage)],
        },
    };
    let (group_y, operation_001) = TestGroup::prepare(group_y, &control_message_001).unwrap();
    let group_y = TestGroup::process(group_y, &operation_001).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![(GroupMember::Individual(alice), Access::Manage)]
    );

    // Add bob with read access.
    let bob = 'B';
    let control_message_002 = GroupControlMessage::GroupAction {
        action: GroupAction::Add {
            group_id: alice,
            member: GroupMember::Individual(bob),
            access: Access::Read,
        },
    };
    let (group_y, operation_002) = TestGroup::prepare(group_y, &control_message_002).unwrap();
    let group_y = TestGroup::process(group_y, &operation_002).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Read)
        ]
    );

    // Add claire with write access.
    let claire = 'C';
    let control_message_003 = GroupControlMessage::GroupAction {
        action: GroupAction::Add {
            group_id: alice,
            member: GroupMember::Individual(claire),
            access: Access::Write,
        },
    };
    let (group_y, operation_003) = TestGroup::prepare(group_y, &control_message_003).unwrap();
    let group_y = TestGroup::process(group_y, &operation_003).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Read),
            (GroupMember::Individual(claire), Access::Write)
        ]
    );

    // Promote claire to admin.
    let control_message_004 = GroupControlMessage::GroupAction {
        action: GroupAction::Promote {
            group_id: alice,
            member: GroupMember::Individual(claire),
            access: Access::Manage,
        },
    };
    let (group_y, operation_004) = TestGroup::prepare(group_y, &control_message_004).unwrap();
    let group_y = TestGroup::process(group_y, &operation_004).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Read),
            (GroupMember::Individual(claire), Access::Manage)
        ]
    );

    // Demote bob to poll access.
    let control_message_005 = GroupControlMessage::GroupAction {
        action: GroupAction::Demote {
            group_id: alice,
            member: GroupMember::Individual(bob),
            access: Access::Pull,
        },
    };
    let (group_y, operation_005) = TestGroup::prepare(group_y, &control_message_005).unwrap();
    let group_y = TestGroup::process(group_y, &operation_005).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Pull),
            (GroupMember::Individual(claire), Access::Manage)
        ]
    );

    // Remove bob.
    let control_message_006 = GroupControlMessage::GroupAction {
        action: GroupAction::Remove {
            group_id: alice,
            member: GroupMember::Individual(bob),
        },
    };
    let (group_y, operation_006) = TestGroup::prepare(group_y, &control_message_006).unwrap();
    let group_y = TestGroup::process(group_y, &operation_006).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(claire), Access::Manage)
        ]
    );
}

#[test]
fn nested_groups() {
    let alice = 'A';
    let bob = 'B';
    let bob_mobile = 'M';
    let bob_laptop = 'L';
    let alice_orderer_y = TestOrdererState::new(alice);
    let bob_orderer_y = TestOrdererState::new(bob);
    let alice_groups = TestMemoryStore::default();
    let bob_groups = TestMemoryStore::default();
    let alice_group_y = GroupState::new(alice, alice, alice_groups, alice_orderer_y);
    let bob_group_y = GroupState::new(bob, alice, bob_groups, bob_orderer_y);

    // Create group with alice as initial admin member.
    let control_message_001 = GroupControlMessage::GroupAction {
        action: GroupAction::Create {
            initial_members: vec![(GroupMember::Individual(alice), Access::Manage)],
        },
    };

    // Prepare the operation on alice's instance.
    let (alice_group_y, operation_001) =
        TestGroup::prepare(alice_group_y, &control_message_001).unwrap();

    // Process it on both alice and bob's instances.
    let alice_group_y = TestGroup::process(alice_group_y, &operation_001).unwrap();
    let bob_group_y = TestGroup::process(bob_group_y, &operation_001).unwrap();

    // Both alice and bob's group instances contain alice as the only (admin) member.
    for y in [&alice_group_y, &bob_group_y] {
        let mut members = y.members();
        members.sort();
        assert_eq!(
            members,
            vec![(GroupMember::Individual(alice), Access::Manage)]
        );
    }

    // Create bobs devices sub-group.
    let control_message_002 = GroupControlMessage::GroupAction {
        action: GroupAction::Create {
            initial_members: vec![
                (GroupMember::Individual(bob_laptop), Access::Manage),
                (GroupMember::Individual(bob_mobile), Access::Write),
            ],
        },
    };

    // Prepare it on bob's instance.
    let (bob_group_y, operation_002) =
        TestGroup::prepare(bob_group_y, &control_message_002).unwrap();

    // Process it on both alice and bob's instances.
    let alice_group_y = TestGroup::process(alice_group_y, &operation_002).unwrap();
    let bob_group_y = TestGroup::process(bob_group_y, &operation_002).unwrap();

    // For both alice and bob's instance it doesn't show up in direct or transitive members yet.
    for y in [&alice_group_y, &bob_group_y] {
        let mut members = y.members();
        members.sort();
        assert_eq!(
            members,
            vec![(GroupMember::Individual(alice), Access::Manage),]
        );
        let mut transitive_members = y.transitive_members();
        transitive_members.sort();
        assert_eq!(transitive_members, vec![(alice, Access::Manage)]);
    }

    // Add bobs devices sub-group as a member with read access.
    let control_message_003 = GroupControlMessage::GroupAction {
        action: GroupAction::Add {
            group_id: alice,
            member: GroupMember::Group { id: bob },
            access: Access::Read,
        },
    };
    let (alice_group_y, operation_003) =
        TestGroup::prepare(alice_group_y, &control_message_003).unwrap();
    let alice_group_y = TestGroup::process(alice_group_y, &operation_003).unwrap();
    let bob_group_y = TestGroup::process(bob_group_y, &operation_003).unwrap();

    for y in [&alice_group_y, &bob_group_y] {
        // Bob's group is a direct member with read access.
        let mut members = y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Group { id: bob }, Access::Read)
            ]
        );

        // Bob's laptop and mobile are transitive members with read access (even though they have
        // higher access levels in bob's device group).
        let mut transitive_members = y.transitive_members();
        transitive_members.sort();
        assert_eq!(
            transitive_members,
            vec![
                (alice, Access::Manage),
                (bob_laptop, Access::Read),
                (bob_mobile, Access::Read)
            ]
        );
    }
}
