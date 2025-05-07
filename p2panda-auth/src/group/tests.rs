use crate::group::GroupState;
use crate::group::test_utils::{TestGroup, TestGroupState, TestGroupStoreState, TestOrdererState};
use crate::traits::AuthGraph;

use super::access::Access;
use super::{GroupAction, GroupControlMessage, GroupMember};

#[test]
fn basic_group() {
    let alice = 'A';
    let group_store_y = TestGroupStoreState::default();
    let orderer_y = TestOrdererState::new(alice, group_store_y.clone());
    let group_y = TestGroupState::new(alice, alice, group_store_y, orderer_y);

    // Create group with alice as initial admin member.
    let control_message_001 = GroupControlMessage::GroupAction {
        group_id: alice,
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
        group_id: alice,
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
        group_id: alice,
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
        group_id: alice,
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
        group_id: alice,
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
        group_id: alice,
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
    let alice = 'A';
    let alice_mobile = 'M';
    let alice_laptop = 'L';

    let alice_devices_group = 'D';
    let alice_team_group = 'T';

    // The group store is shared state across all group instances.
    let group_store_y = TestGroupStoreState::default();
    let alice_orderer_y = TestOrdererState::new(alice, group_store_y.clone());

    // One devices group instance.
    let devices_group_y = GroupState::new(
        alice,
        alice_devices_group,
        group_store_y.clone(),
        alice_orderer_y.clone(),
    );

    // One team group instance.
    let team_group_y = GroupState::new(
        alice,
        alice_team_group,
        group_store_y.clone(),
        alice_orderer_y,
    );

    // Control message creating the devices group, with alice, alice_laptop and alice mobile as members.
    let control_message_001 = GroupControlMessage::GroupAction {
        group_id: devices_group_y.id(),
        action: GroupAction::Create {
            initial_members: vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(alice_laptop), Access::Manage),
                (GroupMember::Individual(alice_mobile), Access::Write),
            ],
        },
    };

    // Prepare the operation.
    let (devices_group_y, operation_001) =
        TestGroup::prepare(devices_group_y, &control_message_001).unwrap();

    // Process the operation.
    let devices_group_y = TestGroup::process(devices_group_y, &operation_001).unwrap();

    // alice, alice_laptop and alice_mobile are all members of the group.
    let mut members = devices_group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(alice_laptop), Access::Manage),
            (GroupMember::Individual(alice_mobile), Access::Write),
        ],
    );

    // Create alice's team group, with alice as the only member.
    let control_message_002 = GroupControlMessage::GroupAction {
        group_id: team_group_y.id(),
        action: GroupAction::Create {
            initial_members: vec![(GroupMember::Individual(alice), Access::Manage)],
        },
    };

    // Prepare the operation.
    let (team_group_y, operation_002) =
        TestGroup::prepare(team_group_y, &control_message_002).unwrap();

    // Process it.
    let team_group_y = TestGroup::process(team_group_y, &operation_002).unwrap();

    // Add alice's devices group as a member of her teams group with read access.
    let control_message_003 = GroupControlMessage::GroupAction {
        group_id: team_group_y.id(),
        action: GroupAction::Add {
            member: GroupMember::Group {
                id: devices_group_y.id(),
            },
            access: Access::Read,
        },
    };
    let (team_group_y, operation_003) =
        TestGroup::prepare(team_group_y, &control_message_003).unwrap();
    let team_group_y = TestGroup::process(team_group_y, &operation_003).unwrap();

    // Alice and the devices group are direct members of the team group.
    let mut members = team_group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (
                GroupMember::Group {
                    id: alice_devices_group
                },
                Access::Read
            )
        ]
    );

    // alice, alice_laptop and alice_mobile are transitive members, only alice has Manage access
    // (even though alice_laptop has Manage access to the devices sub-group).
    let mut transitive_members = team_group_y.transitive_members().unwrap();
    transitive_members.sort();
    assert_eq!(
        transitive_members,
        vec![
            (alice, Access::Manage),
            (alice_laptop, Access::Read),
            (alice_mobile, Access::Read),
        ]
    );
}
