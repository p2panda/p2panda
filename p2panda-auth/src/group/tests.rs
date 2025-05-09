use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::group::GroupState;
use crate::group::test_utils::{
    Network, TestGroup, TestGroupState, TestGroupStoreState, TestOrdererState,
};
use crate::traits::AuthGraph;

use super::access::Access;
use super::{GroupAction, GroupControlMessage, GroupMember};

#[test]
fn basic_group() {
    let alice = 'A';
    let group_store_y = TestGroupStoreState::default();
    let rng = StdRng::from_os_rng();
    let orderer_y = TestOrdererState::new(alice, group_store_y.clone(), rng);
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
    let rng = StdRng::from_os_rng();
    let alice_orderer_y = TestOrdererState::new(alice, group_store_y.clone(), rng);

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

#[test]
fn multi_user() {
    let alice = 'A';
    let bob = 'B';
    let claire = 'C';

    let alice_mobile = 'M';
    let alice_laptop = 'L';

    let alice_devices_group = 'D';
    let alice_team_group = 'T';

    let rng = StdRng::from_os_rng();
    // let rng = StdRng::from_seed([0u8; 32]);

    let mut network = Network::new([alice, bob, claire], rng);

    // Alice creates a team group with themselves as initial member.
    network.create(
        alice_team_group,
        alice,
        vec![(GroupMember::Individual(alice), Access::Manage)],
    );

    // And then adds bob as manager.
    network.add(
        alice,
        GroupMember::Individual(bob),
        alice_team_group,
        Access::Manage,
    );

    // Everyone processes these operations.
    network.process();

    let alice_members = network.members(&alice, &alice_team_group);
    let bob_members = network.members(&bob, &alice_team_group);
    let claire_members = network.members(&claire, &alice_team_group);
    assert_eq!(
        alice_members,
        vec![
            (GroupMember::Individual('A'), Access::Manage),
            (GroupMember::Individual('B'), Access::Manage),
        ]
    );
    assert_eq!(alice_members, claire_members);
    assert_eq!(alice_members, bob_members);

    let alice_transitive_members = network.transitive_members(&alice, &alice_team_group);
    let bob_transitive_members = network.transitive_members(&bob, &alice_team_group);
    let claire_transitive_members = network.transitive_members(&claire, &alice_team_group);
    assert_eq!(
        alice_transitive_members,
        vec![('A', Access::Manage), ('B', Access::Manage),]
    );
    assert_eq!(alice_transitive_members, bob_transitive_members);
    assert_eq!(alice_transitive_members, claire_transitive_members);

    // Bob adds claire with read access.
    network.add(
        bob,
        GroupMember::Individual(claire),
        alice_team_group,
        Access::Read,
    );

    // Alice (concurrently) creates a devices group.
    network.create(
        alice_devices_group,
        alice,
        vec![
            (GroupMember::Individual(alice_mobile), Access::Write),
            (GroupMember::Individual(alice_laptop), Access::Manage),
        ],
    );

    // And adds it to the teams group.
    network.add(
        alice,
        GroupMember::Group {
            id: alice_devices_group,
        },
        alice_team_group,
        Access::Manage,
    );

    // Everyone processes these operations.
    network.process();

    // alice, bob and claire now
    let alice_members = network.members(&alice, &alice_team_group);
    let bob_members = network.members(&bob, &alice_team_group);
    let claire_members = network.members(&claire, &alice_team_group);
    assert_eq!(
        alice_members,
        vec![
            (GroupMember::Individual('A'), Access::Manage),
            (GroupMember::Individual('B'), Access::Manage),
            (GroupMember::Individual('C'), Access::Read),
            (GroupMember::Group { id: 'D' }, Access::Manage)
        ]
    );
    assert_eq!(alice_members, bob_members);
    assert_eq!(alice_members, claire_members);

    let alice_transitive_members = network.transitive_members(&alice, &alice_team_group);
    let bob_transitive_members = network.transitive_members(&bob, &alice_team_group);
    let claire_transitive_members = network.transitive_members(&claire, &alice_team_group);
    assert_eq!(
        alice_transitive_members,
        vec![
            ('A', Access::Manage),
            ('B', Access::Manage),
            ('C', Access::Read),
            ('L', Access::Manage),
            ('M', Access::Write)
        ]
    );
    assert_eq!(alice_transitive_members, bob_transitive_members);
    assert_eq!(alice_transitive_members, claire_transitive_members);
}

#[test]
fn ooo() {
    let alice = 'A';
    let bob = 'B';
    let claire = 'C';

    let alice_friends = vec!['D', 'E', 'F'];
    let bob_friends = vec!['G', 'H', 'I'];
    let claire_friends = vec!['J', 'K', 'L'];

    let friends_group = 'T';

    let rng = StdRng::from_os_rng();
    // let rng = StdRng::from_seed([0u8; 32]);

    let mut network = Network::new([alice, bob, claire], rng);

    // Alice creates a team group with themselves as initial member.
    network.create(
        friends_group,
        alice,
        vec![
            (GroupMember::Individual(alice), Access::Manage),
            (GroupMember::Individual(bob), Access::Manage),
            (GroupMember::Individual(claire), Access::Manage),
        ],
    );

    network.process();

    for friend in &alice_friends {
        network.add(
            alice,
            GroupMember::Individual(*friend),
            friends_group,
            Access::Read,
        );
    }

    network.remove(
        alice,
        GroupMember::Individual(alice_friends[0]),
        friends_group,
    );

    for friend in &bob_friends {
        network.add(
            bob,
            GroupMember::Individual(*friend),
            friends_group,
            Access::Read,
        );
    }

    network.remove(bob, GroupMember::Individual(bob_friends[0]), friends_group);

    for friend in &claire_friends {
        network.add(
            claire,
            GroupMember::Individual(*friend),
            friends_group,
            Access::Read,
        );
    }

    network.remove(
        claire,
        GroupMember::Individual(claire_friends[0]),
        friends_group,
    );

    network.process_ooo();

    let alice_members = network.members(&alice, &friends_group);
    let bob_members = network.members(&bob, &friends_group);
    let claire_members = network.members(&claire, &friends_group);
    assert_eq!(
        alice_members,
        vec![
            (GroupMember::Individual('A'), Access::Manage),
            (GroupMember::Individual('B'), Access::Manage),
            (GroupMember::Individual('C'), Access::Manage),
            // (GroupMember::Individual('D'), Access::Read),
            (GroupMember::Individual('E'), Access::Read),
            (GroupMember::Individual('F'), Access::Read),
            // (GroupMember::Individual('G'), Access::Read),
            (GroupMember::Individual('H'), Access::Read),
            (GroupMember::Individual('I'), Access::Read),
            // (GroupMember::Individual('J'), Access::Read),
            (GroupMember::Individual('K'), Access::Read),
            (GroupMember::Individual('L'), Access::Read),
        ]
    );
    assert_eq!(alice_members, claire_members);
    assert_eq!(alice_members, bob_members);
}
