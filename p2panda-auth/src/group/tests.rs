use crate::group::test_utils::{TestOrderer, TestOrdererState};
use crate::group::{Group, GroupState};
use crate::traits::{AuthGraph, IdentityHandle, OperationId, Ordering};

use super::access::Access;
use super::resolver::GroupResolver;
use super::test_utils::TestOperation;
use super::{GroupAction, GroupControlMessage, GroupMember};

impl IdentityHandle for char {}
impl OperationId for u32 {}

type TestResolver = GroupResolver<char, u32, TestOperation<char, u32>>;
type TestGroup = Group<char, u32, TestResolver, TestOrderer>;

#[test]
fn basic_group() {
    let alice = 'A';
    let group_id = 'X';
    let orderer_y = TestOrdererState {
        my_id: alice,
        operations: Default::default(),
    };
    let group_y = GroupState::new(alice, group_id, orderer_y);

    // Create group with alice as initial admin member.
    let control_message_001 = GroupControlMessage::GroupAction {
        group_id,
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
        group_id,
        action: GroupAction::Add {
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
        group_id,
        action: GroupAction::Add {
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
        group_id,
        action: GroupAction::Promote {
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
        group_id,
        action: GroupAction::Demote {
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
        group_id,
        action: GroupAction::Remove {
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
    let root_group_id = '0';
    let alice = 'A';
    let bob_devices_group_id = 'B';
    let bob_mobile = 'M';
    let bob_laptop = 'L';
    let orderer_y = TestOrdererState {
        my_id: alice,
        operations: Default::default(),
    };
    let group_y = GroupState::new(alice, root_group_id, orderer_y);

    // Create group with alice as initial admin member.
    let control_message_001 = GroupControlMessage::GroupAction {
        group_id: root_group_id,
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

    // Create bobs devices sub-group.
    let control_message_002 = GroupControlMessage::GroupAction {
        group_id: bob_devices_group_id,
        action: GroupAction::Create {
            initial_members: vec![
                (GroupMember::Individual(bob_laptop), Access::Manage),
                (GroupMember::Individual(bob_mobile), Access::Write),
            ],
        },
    };
    let (group_y, operation_002) = TestGroup::prepare(group_y, &control_message_002).unwrap();
    let group_y = TestGroup::process(group_y, &operation_002).unwrap();

    // It doesn't show up in direct or transitive members yet.
    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![(GroupMember::Individual(alice), Access::Manage),]
    );
    let mut transitive_members = group_y.transitive_members();
    transitive_members.sort();
    assert_eq!(transitive_members, vec![(alice, Access::Manage)]);

    // Add bobs devices sub-group as a member with read access.
    let control_message_003 = GroupControlMessage::GroupAction {
        group_id: root_group_id,
        action: GroupAction::Add {
            member: GroupMember::Group {
                id: bob_devices_group_id,
            },
            access: Access::Read,
        },
    };
    let (group_y, operation_003) = TestGroup::prepare(group_y, &control_message_003).unwrap();
    let group_y = TestGroup::process(group_y, &operation_003).unwrap();

    // Bob's group is a direct member with read access.
    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (
                GroupMember::Group {
                    id: bob_devices_group_id
                },
                Access::Read
            )
        ]
    );

    // Bob's laptop and mobile are transitive members with read access (even though they have
    // higher access levels in bob's device group).
    let mut transitive_members = group_y.transitive_members();
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
