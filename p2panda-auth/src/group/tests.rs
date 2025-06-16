// SPDX-License-Identifier: MIT OR Apache-2.0

use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::group::Access;
use crate::group::GroupState;
use crate::group::test_utils::TestGroupStore;
use crate::group::test_utils::{Network, TestGroup, TestGroupState, TestOrdererState};
use crate::traits::AuthGroup;

use super::test_utils::MessageId;
use super::{GroupAction, GroupControlMessage, GroupMember};

#[test]
fn basic_group() {
    let alice = 'A';
    let store = TestGroupStore::default();
    let rng = StdRng::from_os_rng();
    let orderer_y = TestOrdererState::new(alice, store.clone(), rng);
    let group_y = TestGroupState::new(alice, alice, store, orderer_y);

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
            access: Access::Write { conditions: None },
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
            (
                GroupMember::Individual(claire),
                Access::Write { conditions: None }
            )
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
    let store = TestGroupStore::default();
    let rng = StdRng::from_os_rng();
    let alice_orderer_y = TestOrdererState::new(alice, store.clone(), rng);

    // One devices group instance.
    let devices_group_y = GroupState::new(
        alice,
        alice_devices_group,
        store.clone(),
        alice_orderer_y.clone(),
    );

    // One team group instance.
    let team_group_y = GroupState::new(alice, alice_team_group, store.clone(), alice_orderer_y);

    // Control message creating the devices group, with alice, alice_laptop and alice mobile as members.
    let control_message_001 = GroupControlMessage::GroupAction {
        group_id: devices_group_y.id(),
        action: GroupAction::Create {
            initial_members: vec![
                (GroupMember::Individual(alice), Access::Manage),
                (GroupMember::Individual(alice_laptop), Access::Manage),
                (
                    GroupMember::Individual(alice_mobile),
                    Access::Write { conditions: None },
                ),
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
            (
                GroupMember::Individual(alice_mobile),
                Access::Write { conditions: None }
            ),
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
            member: GroupMember::Group(devices_group_y.id()),
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
            (GroupMember::Group(alice_devices_group), Access::Read)
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
            (
                GroupMember::Individual(alice_mobile),
                Access::Write { conditions: None },
            ),
            (GroupMember::Individual(alice_laptop), Access::Manage),
        ],
    );

    // And adds it to the teams group.
    network.add(
        alice,
        GroupMember::Group(alice_devices_group),
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
            (GroupMember::Individual('C'), Access::Read),
            (GroupMember::Group('D'), Access::Manage)
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
            ('M', Access::Write { conditions: None })
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

    // Alice creates a friends group with themselves as initial member.
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

    // alice, bob and claire all concurrently add 3 new friends, then remove one
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

    // alice, bob and claire all process these messages in random orders.
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

#[test]
fn add_remove_add() {
    let alice = 'A';
    let bob = 'B';

    let friends_group = 'T';

    let rng = StdRng::from_os_rng();
    // let rng = StdRng::from_seed([0u8; 32]);

    let mut network = Network::new([alice, bob], rng);

    network.create(
        friends_group,
        alice,
        vec![(GroupMember::Individual(alice), Access::Manage)],
    );

    network.add(
        alice,
        GroupMember::Individual(bob),
        friends_group,
        Access::Read,
    );

    network.remove(alice, GroupMember::Individual(bob), friends_group);

    let members = network.members(&alice, &friends_group);
    assert_eq!(
        members,
        vec![(GroupMember::Individual('A'), Access::Manage),]
    );

    network.add(
        alice,
        GroupMember::Individual(bob),
        friends_group,
        Access::Read,
    );

    network.process();

    let members = network.members(&alice, &friends_group);
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual('A'), Access::Manage),
            (GroupMember::Individual('B'), Access::Read),
        ]
    );
}

const ALICE: char = 'A';
const BOB: char = 'B';
const CHARLIE: char = 'C';
const EDITH: char = 'E';
const BOB_MOBILE: char = 'M';
const BOB_LAPTOP: char = 'L';

const BOB_DEVICES_GROUP: char = 'D';
const CHARLIE_TEAM_GROUP: char = 'T';
const ALICE_ORG_GROUP: char = 'O';

// No concurrency in these test groups, the group store and orderer are shared across all group
// instances.
fn test_groups(rng: StdRng) -> (Network, Vec<MessageId>) {
    let mut network = Network::new([ALICE, BOB, CHARLIE], rng);
    let mut operations = vec![];

    let id = network.create(
        BOB_DEVICES_GROUP,
        BOB,
        vec![
            (GroupMember::Individual(BOB), Access::Manage),
            (
                GroupMember::Individual(BOB_LAPTOP),
                Access::Write { conditions: None },
            ),
        ],
    );
    operations.push(id);

    let id = network.add(
        BOB,
        GroupMember::Individual(BOB_MOBILE),
        BOB_DEVICES_GROUP,
        Access::Read,
    );
    operations.push(id);

    network.process();

    let id = network.create(
        CHARLIE_TEAM_GROUP,
        CHARLIE,
        vec![
            (GroupMember::Individual(CHARLIE), Access::Manage),
            (GroupMember::Individual(EDITH), Access::Read),
        ],
    );
    operations.push(id);

    let id = network.create(
        ALICE_ORG_GROUP,
        ALICE,
        vec![(GroupMember::Individual(ALICE), Access::Manage)],
    );
    operations.push(id);

    network.process();

    let id = network.add(
        CHARLIE,
        GroupMember::Group(BOB_DEVICES_GROUP),
        CHARLIE_TEAM_GROUP,
        Access::Manage,
    );
    operations.push(id);

    network.process();

    let id = network.add(
        ALICE,
        GroupMember::Group(CHARLIE_TEAM_GROUP),
        ALICE_ORG_GROUP,
        Access::Write { conditions: None },
    );
    operations.push(id);

    network.process();

    (network, operations)
}

#[test]
fn transitive_members() {
    let rng = StdRng::from_os_rng();
    let (network, _) = test_groups(rng);

    let expected_bob_devices_group_direct_members = vec![
        (GroupMember::Individual(BOB), Access::Manage),
        (
            GroupMember::Individual(BOB_LAPTOP),
            Access::Write { conditions: None },
        ),
        (GroupMember::Individual(BOB_MOBILE), Access::Read),
    ];

    let expected_bob_devices_group_transitive_members = vec![
        (BOB, Access::Manage),
        (BOB_LAPTOP, Access::Write { conditions: None }),
        (BOB_MOBILE, Access::Read),
    ];

    let expected_charlie_team_group_direct_members = vec![
        (GroupMember::Individual(CHARLIE), Access::Manage),
        (GroupMember::Individual(EDITH), Access::Read),
        (GroupMember::Group(BOB_DEVICES_GROUP), Access::Manage),
    ];

    let expected_charlie_team_group_transitive_members = vec![
        (BOB, Access::Manage),
        (CHARLIE, Access::Manage),
        (EDITH, Access::Read),
        (BOB_LAPTOP, Access::Write { conditions: None }),
        (BOB_MOBILE, Access::Read),
    ];

    let expected_alice_org_group_direct_members = vec![
        (GroupMember::Individual(ALICE), Access::Manage),
        (
            GroupMember::Group(CHARLIE_TEAM_GROUP),
            Access::Write { conditions: None },
        ),
    ];

    let expected_alice_org_group_transitive_members = vec![
        (ALICE, Access::Manage),
        (BOB, Access::Write { conditions: None }),
        (CHARLIE, Access::Write { conditions: None }),
        (EDITH, Access::Read),
        (BOB_LAPTOP, Access::Write { conditions: None }),
        (BOB_MOBILE, Access::Read),
    ];

    let members = network.members(&BOB, &BOB_DEVICES_GROUP);
    assert_eq!(members, expected_bob_devices_group_direct_members);

    let transitive_members = network.transitive_members(&BOB, &BOB_DEVICES_GROUP);
    assert_eq!(
        transitive_members,
        expected_bob_devices_group_transitive_members
    );

    let members = network.members(&CHARLIE, &CHARLIE_TEAM_GROUP);
    assert_eq!(members, expected_charlie_team_group_direct_members);

    let transitive_members = network.transitive_members(&CHARLIE, &CHARLIE_TEAM_GROUP);
    assert_eq!(
        transitive_members,
        expected_charlie_team_group_transitive_members
    );

    let members = network.members(&ALICE, &ALICE_ORG_GROUP);
    assert_eq!(members, expected_alice_org_group_direct_members);

    let transitive_members = network.transitive_members(&ALICE, &ALICE_ORG_GROUP);
    assert_eq!(
        transitive_members,
        expected_alice_org_group_transitive_members
    );
}

#[test]
fn members_at() {
    let rng = StdRng::from_os_rng();
    let (network, operations) = test_groups(rng);

    let create_devices_op_id = operations[0];
    let add_mobile_to_devices_op_id = operations[1];
    let create_team_op_id = operations[2];
    let create_org_op_id = operations[3];
    let add_devices_to_team_op_id = operations[4];
    let add_team_to_org_op_id = operations[5];

    // Initial state of the org group.
    let members = network.transitive_members_at(&ALICE, &ALICE_ORG_GROUP, &vec![create_org_op_id]);
    assert_eq!(members, vec![(ALICE, Access::Manage)]);

    // CHARLIE_TEAM was added but before BOB_DEVICES was added to the team.
    let members = network.transitive_members_at(
        &ALICE,
        &ALICE_ORG_GROUP,
        &vec![add_team_to_org_op_id, create_team_op_id],
    );
    assert_eq!(
        members,
        vec![
            (ALICE, Access::Manage),
            (CHARLIE, Access::Write { conditions: None }),
            (EDITH, Access::Read)
        ]
    );

    // now BOB_DEVICES was added to the team.
    let members = network.transitive_members_at(
        &ALICE,
        &ALICE_ORG_GROUP,
        &vec![
            add_team_to_org_op_id,
            create_devices_op_id,
            add_devices_to_team_op_id,
        ],
    );
    assert_eq!(
        members,
        vec![
            (ALICE, Access::Manage),
            (BOB, Access::Write { conditions: None }),
            (CHARLIE, Access::Write { conditions: None }),
            (EDITH, Access::Read),
            (BOB_LAPTOP, Access::Write { conditions: None }),
        ]
    );

    // now BOB_MOBILE was added to the devices group and we are at "current state".
    let members_at_most_recent_heads = network.transitive_members_at(
        &ALICE,
        &ALICE_ORG_GROUP,
        &vec![
            add_team_to_org_op_id,
            add_mobile_to_devices_op_id,
            add_devices_to_team_op_id,
        ],
    );
    assert_eq!(
        members_at_most_recent_heads,
        vec![
            (ALICE, Access::Manage),
            (BOB, Access::Write { conditions: None }),
            (CHARLIE, Access::Write { conditions: None }),
            (EDITH, Access::Read),
            (BOB_LAPTOP, Access::Write { conditions: None }),
            (BOB_MOBILE, Access::Read),
        ]
    );

    // These queries should produce the same "current" member state.
    let current_members = network.transitive_members(&ALICE, &ALICE_ORG_GROUP);
    // This is a slightly strange thing to do, we are requesting the current state by passing in a
    // vec of all known operation ids. Logically it should produce the same state though.
    let members_by_all_known_operations =
        network.transitive_members_at(&ALICE, &ALICE_ORG_GROUP, &operations);

    assert_eq!(members_at_most_recent_heads, current_members);
    assert_eq!(
        members_at_most_recent_heads,
        members_by_all_known_operations
    );
}
