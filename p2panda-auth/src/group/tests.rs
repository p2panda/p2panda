// SPDX-License-Identifier: MIT OR Apache-2.0

use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::group::Access;
use crate::group::Group;
use crate::group::GroupError;
use crate::group::GroupMembershipError;
use crate::group::GroupState;
use crate::test_utils::TestGroupStore;
use crate::test_utils::TestOperation;
use crate::test_utils::{Network, TestGroup, TestGroupState, TestOrdererState};
use crate::traits::AuthGroup;
use crate::test_utils::MessageId;
use crate::group::{GroupAction, GroupControlMessage, GroupMember};

pub(crate) fn from_create(
    actor_id: char,
    group_id: char,
    op_create: &TestOperation,
    rng: &mut StdRng,
) -> TestGroupState {
    let store = TestGroupStore::default();
    let orderer = TestOrdererState::new(actor_id, store.clone(), StdRng::from_rng(rng));
    let group = TestGroupState::new(actor_id, group_id, store, orderer);
    TestGroup::process(group, op_create).unwrap()
}

pub(crate) fn create_group(
    actor_id: char,
    group_id: char,
    members: Vec<(char, Access<()>)>,
    rng: &mut StdRng,
) -> (TestGroupState, TestOperation) {
    let store = TestGroupStore::default();
    let orderer = TestOrdererState::new(actor_id, store.clone(), StdRng::from_rng(rng));
    let group = TestGroupState::new(actor_id, group_id, store, orderer);
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

pub(crate) fn add_member(
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

pub(crate) fn remove_member(
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

pub(crate) fn sync(group: TestGroupState, ops: &[TestOperation]) -> TestGroupState {
    ops.iter()
        .fold(group, |g, op| TestGroup::process(g, op).unwrap())
}

pub(crate) fn assert_members(group: &TestGroupState, expected: &[(GroupMember<char>, Access<()>)]) {
    let mut actual = group.members();
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
fn basic_group() {
    let group_id = '1';
    let alice = 'A';
    let store = TestGroupStore::default();
    let rng = StdRng::from_os_rng();
    let orderer_y = TestOrdererState::new(alice, store.clone(), rng);
    let group_y = TestGroupState::new(alice, group_id, store, orderer_y);

    // Create group with alice as initial admin member.
    let control_message_001 = GroupControlMessage::GroupAction {
        group_id,
        action: GroupAction::Create {
            initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
        },
    };
    let (group_y, operation_001) = TestGroup::prepare(group_y, &control_message_001).unwrap();
    let group_y = TestGroup::process(group_y, &operation_001).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![(GroupMember::Individual(alice), Access::manage())]
    );

    // Add bob with read access.
    let bob = 'B';
    let control_message_002 = GroupControlMessage::GroupAction {
        group_id,
        action: GroupAction::Add {
            member: GroupMember::Individual(bob),
            access: Access::read(),
        },
    };
    let (group_y, operation_002) = TestGroup::prepare(group_y, &control_message_002).unwrap();
    let group_y = TestGroup::process(group_y, &operation_002).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::read())
        ]
    );

    // Add claire with write access.
    let claire = 'C';
    let control_message_003 = GroupControlMessage::GroupAction {
        group_id,
        action: GroupAction::Add {
            member: GroupMember::Individual(claire),
            access: Access::write(),
        },
    };
    let (group_y, operation_003) = TestGroup::prepare(group_y, &control_message_003).unwrap();
    let group_y = TestGroup::process(group_y, &operation_003).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::read()),
            (GroupMember::Individual(claire), Access::write())
        ]
    );

    // Promote claire to admin.
    let control_message_004 = GroupControlMessage::GroupAction {
        group_id,
        action: GroupAction::Promote {
            member: GroupMember::Individual(claire),
            access: Access::manage(),
        },
    };
    let (group_y, operation_004) = TestGroup::prepare(group_y, &control_message_004).unwrap();
    let group_y = TestGroup::process(group_y, &operation_004).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::read()),
            (GroupMember::Individual(claire), Access::manage())
        ]
    );

    // Demote bob to poll access.
    let control_message_005 = GroupControlMessage::GroupAction {
        group_id,
        action: GroupAction::Demote {
            member: GroupMember::Individual(bob),
            access: Access::pull(),
        },
    };
    let (group_y, operation_005) = TestGroup::prepare(group_y, &control_message_005).unwrap();
    let group_y = TestGroup::process(group_y, &operation_005).unwrap();

    let mut members = group_y.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::pull()),
            (GroupMember::Individual(claire), Access::manage())
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
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(claire), Access::manage())
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
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(alice_laptop), Access::manage()),
                (GroupMember::Individual(alice_mobile), Access::write()),
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
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(alice_laptop), Access::manage()),
            (GroupMember::Individual(alice_mobile), Access::write()),
        ],
    );

    // Create alice's team group, with alice as the only member.
    let control_message_002 = GroupControlMessage::GroupAction {
        group_id: team_group_y.id(),
        action: GroupAction::Create {
            initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
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
            access: Access::read(),
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
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Group(alice_devices_group), Access::read())
        ]
    );

    // alice, alice_laptop and alice_mobile are transitive members, only alice has Manage access
    // (even though alice_laptop has Manage access to the devices sub-group).
    let mut transitive_members = team_group_y.transitive_members().unwrap();
    transitive_members.sort();
    assert_eq!(
        transitive_members,
        vec![
            (alice, Access::manage()),
            (alice_laptop, Access::read()),
            (alice_mobile, Access::read()),
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
        vec![(GroupMember::Individual(alice), Access::manage())],
    );

    // And then adds bob as manager.
    network.add(
        alice,
        GroupMember::Individual(bob),
        alice_team_group,
        Access::manage(),
    );

    // Everyone processes these operations.
    network.process();

    let alice_members = network.members(&alice, &alice_team_group);
    let bob_members = network.members(&bob, &alice_team_group);
    let claire_members = network.members(&claire, &alice_team_group);
    assert_eq!(
        alice_members,
        vec![
            (GroupMember::Individual('A'), Access::manage()),
            (GroupMember::Individual('B'), Access::manage()),
        ]
    );
    assert_eq!(alice_members, claire_members);
    assert_eq!(alice_members, bob_members);

    let alice_transitive_members = network.transitive_members(&alice, &alice_team_group);
    let bob_transitive_members = network.transitive_members(&bob, &alice_team_group);
    let claire_transitive_members = network.transitive_members(&claire, &alice_team_group);
    assert_eq!(
        alice_transitive_members,
        vec![('A', Access::manage()), ('B', Access::manage()),]
    );
    assert_eq!(alice_transitive_members, bob_transitive_members);
    assert_eq!(alice_transitive_members, claire_transitive_members);

    // Bob adds claire with read access.
    network.add(
        bob,
        GroupMember::Individual(claire),
        alice_team_group,
        Access::read(),
    );

    // Alice (concurrently) creates a devices group.
    network.create(
        alice_devices_group,
        alice,
        vec![
            (GroupMember::Individual(alice_mobile), Access::write()),
            (GroupMember::Individual(alice_laptop), Access::manage()),
        ],
    );

    // And adds it to the teams group.
    network.add(
        alice,
        GroupMember::Group(alice_devices_group),
        alice_team_group,
        Access::manage(),
    );

    // Everyone processes these operations.
    network.process();

    let alice_members = network.members(&alice, &alice_team_group);
    let bob_members = network.members(&bob, &alice_team_group);
    let claire_members = network.members(&claire, &alice_team_group);
    assert_eq!(
        alice_members,
        vec![
            (GroupMember::Individual('A'), Access::manage()),
            (GroupMember::Individual('B'), Access::manage()),
            (GroupMember::Individual('C'), Access::read()),
            (GroupMember::Group('D'), Access::manage())
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
            ('A', Access::manage()),
            ('B', Access::manage()),
            ('C', Access::read()),
            ('L', Access::manage()),
            ('M', Access::write())
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
            (GroupMember::Individual(alice), Access::manage()),
            (GroupMember::Individual(bob), Access::manage()),
            (GroupMember::Individual(claire), Access::manage()),
        ],
    );

    network.process();

    // alice, bob and claire all concurrently add 3 new friends, then remove one
    for friend in &alice_friends {
        network.add(
            alice,
            GroupMember::Individual(*friend),
            friends_group,
            Access::read(),
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
            Access::read(),
        );
    }

    network.remove(bob, GroupMember::Individual(bob_friends[0]), friends_group);

    for friend in &claire_friends {
        network.add(
            claire,
            GroupMember::Individual(*friend),
            friends_group,
            Access::read(),
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
            (GroupMember::Individual('A'), Access::manage()),
            (GroupMember::Individual('B'), Access::manage()),
            (GroupMember::Individual('C'), Access::manage()),
            // (GroupMember::Individual('D'), Access::read()),
            (GroupMember::Individual('E'), Access::read()),
            (GroupMember::Individual('F'), Access::read()),
            // (GroupMember::Individual('G'), Access::read()),
            (GroupMember::Individual('H'), Access::read()),
            (GroupMember::Individual('I'), Access::read()),
            // (GroupMember::Individual('J'), Access::read()),
            (GroupMember::Individual('K'), Access::read()),
            (GroupMember::Individual('L'), Access::read()),
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
        vec![(GroupMember::Individual(alice), Access::manage())],
    );

    network.add(
        alice,
        GroupMember::Individual(bob),
        friends_group,
        Access::read(),
    );

    network.remove(alice, GroupMember::Individual(bob), friends_group);

    let members = network.members(&alice, &friends_group);
    assert_eq!(
        members,
        vec![(GroupMember::Individual('A'), Access::manage()),]
    );

    network.add(
        alice,
        GroupMember::Individual(bob),
        friends_group,
        Access::read(),
    );

    network.process();

    let members = network.members(&alice, &friends_group);
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual('A'), Access::manage()),
            (GroupMember::Individual('B'), Access::read()),
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
            (GroupMember::Individual(BOB), Access::manage()),
            (GroupMember::Individual(BOB_LAPTOP), Access::write()),
        ],
    );
    operations.push(id);

    let id = network.add(
        BOB,
        GroupMember::Individual(BOB_MOBILE),
        BOB_DEVICES_GROUP,
        Access::read(),
    );
    operations.push(id);

    network.process();

    let id = network.create(
        CHARLIE_TEAM_GROUP,
        CHARLIE,
        vec![
            (GroupMember::Individual(CHARLIE), Access::manage()),
            (GroupMember::Individual(EDITH), Access::read()),
        ],
    );
    operations.push(id);

    let id = network.create(
        ALICE_ORG_GROUP,
        ALICE,
        vec![(GroupMember::Individual(ALICE), Access::manage())],
    );
    operations.push(id);

    network.process();

    let id = network.add(
        CHARLIE,
        GroupMember::Group(BOB_DEVICES_GROUP),
        CHARLIE_TEAM_GROUP,
        Access::manage(),
    );
    operations.push(id);

    network.process();

    let id = network.add(
        ALICE,
        GroupMember::Group(CHARLIE_TEAM_GROUP),
        ALICE_ORG_GROUP,
        Access::write(),
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
        (GroupMember::Individual(BOB), Access::manage()),
        (GroupMember::Individual(BOB_LAPTOP), Access::write()),
        (GroupMember::Individual(BOB_MOBILE), Access::read()),
    ];

    let expected_bob_devices_group_transitive_members = vec![
        (BOB, Access::manage()),
        (BOB_LAPTOP, Access::write()),
        (BOB_MOBILE, Access::read()),
    ];

    let expected_charlie_team_group_direct_members = vec![
        (GroupMember::Individual(CHARLIE), Access::manage()),
        (GroupMember::Individual(EDITH), Access::read()),
        (GroupMember::Group(BOB_DEVICES_GROUP), Access::manage()),
    ];

    let expected_charlie_team_group_transitive_members = vec![
        (BOB, Access::manage()),
        (CHARLIE, Access::manage()),
        (EDITH, Access::read()),
        (BOB_LAPTOP, Access::write()),
        (BOB_MOBILE, Access::read()),
    ];

    let expected_alice_org_group_direct_members = vec![
        (GroupMember::Individual(ALICE), Access::manage()),
        (GroupMember::Group(CHARLIE_TEAM_GROUP), Access::write()),
    ];

    let expected_alice_org_group_transitive_members = vec![
        (ALICE, Access::manage()),
        (BOB, Access::write()),
        (CHARLIE, Access::write()),
        (EDITH, Access::read()),
        (BOB_LAPTOP, Access::write()),
        (BOB_MOBILE, Access::read()),
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
    assert_eq!(members, vec![(ALICE, Access::manage())]);

    // CHARLIE_TEAM was added but before BOB_DEVICES was added to the team.
    let members = network.transitive_members_at(
        &ALICE,
        &ALICE_ORG_GROUP,
        &vec![add_team_to_org_op_id, create_team_op_id],
    );
    assert_eq!(
        members,
        vec![
            (ALICE, Access::manage()),
            (CHARLIE, Access::write()),
            (EDITH, Access::read())
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
            (ALICE, Access::manage()),
            (BOB, Access::write()),
            (CHARLIE, Access::write()),
            (EDITH, Access::read()),
            (BOB_LAPTOP, Access::write()),
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
            (ALICE, Access::manage()),
            (BOB, Access::write()),
            (CHARLIE, Access::write()),
            (EDITH, Access::read()),
            (BOB_LAPTOP, Access::write()),
            (BOB_MOBILE, Access::read()),
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
#[test]
fn error_cases() {
    let group_id = '0';
    let alice = 'A';
    let bob = 'B';
    let claire = 'C';
    let dave = 'D';
    let eve = 'E';

    let mut rng = StdRng::from_os_rng();

    let (y_i, _) = create_group(
        alice,
        group_id,
        vec![
            (alice, Access::manage()),
            (bob, Access::read()),
            (claire, Access::read()),
        ],
        &mut rng,
    );

    let previous: Vec<u32> = y_i.heads().into_iter().collect();

    // AlreadyAdded
    let op = TestOperation {
        id: 1,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(bob),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_i.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::AlreadyAdded(GroupMember::Individual('B'))
        ))
    ));

    // Remove claire so we can test AlreadyRemoved
    let y_ii = Group::process(
        y_i,
        &TestOperation {
            id: 2,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage::GroupAction {
                group_id,
                action: GroupAction::Remove {
                    member: GroupMember::Individual(claire),
                },
            },
        },
    )
    .unwrap();

    let previous: Vec<u32> = y_ii.heads().into_iter().collect();

    // AlreadyRemoved
    let op = TestOperation {
        id: 3,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(claire),
            },
        },
    };
    assert!(matches!(
        Group::process(y_ii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::AlreadyRemoved(GroupMember::Individual('C'))
        ))
    ));

    // InsufficientAccess
    let op = TestOperation {
        id: 4,
        author: bob,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_ii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::InsufficientAccess(GroupMember::Individual('B'))
        ))
    ));

    // Remove bob so we can test InactiveActor
    let y_iii = Group::process(
        y_ii,
        &TestOperation {
            id: 5,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage::GroupAction {
                group_id,
                action: GroupAction::Remove {
                    member: GroupMember::Individual(bob),
                },
            },
        },
    )
    .unwrap();

    let previous: Vec<u32> = y_iii.heads().into_iter().collect();

    // InactiveActor
    let op = TestOperation {
        id: 6,
        author: bob,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::InactiveActor(GroupMember::Individual('B'))
        ))
    ));

    // InactiveMember
    let op = TestOperation {
        id: 7,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(claire),
                access: Access::write(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::InactiveMember(GroupMember::Individual('C'))
        ))
    ));

    // UnrecognisedActor
    let op = TestOperation {
        id: 8,
        author: eve,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::UnrecognisedActor(GroupMember::Individual('E'))
        ))
    ));

    // UnrecognisedMember
    let op = TestOperation {
        id: 9,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(eve),
                access: Access::write(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::UnrecognisedMember(GroupMember::Individual('E'))
        ))
    ));
}
#[test]
fn error_cases_resolver() {
    let group_id = '0';
    let alice = 'A';
    let bob = 'B';
    let claire = 'C';
    let dave = 'D';
    let eve = 'E';

    let mut rng = StdRng::from_os_rng();

    let (y_i, _) = create_group(
        alice,
        group_id,
        vec![
            (alice, Access::manage()),
            (bob, Access::read()),
            (claire, Access::read()),
        ],
        &mut rng,
    );

    let previous: Vec<u32> = y_i.heads().into_iter().collect();

    // Remove all current members and all all non-members as managers in a concurrent branch.
    let (mut y_ii, _) = remove_member(y_i, group_id, bob);
    (y_ii, _) = remove_member(y_ii, group_id, claire);
    (y_ii, _) = add_member(y_ii, group_id, dave, Access::manage());
    (y_ii, _) = add_member(y_ii, group_id, eve, Access::manage());
    (y_ii, _) = remove_member(y_ii, group_id, alice);

    let mut members = y_ii.members();
    members.sort();
    assert_eq!(
        members,
        vec![
            (GroupMember::Individual(dave), Access::manage()),
            (GroupMember::Individual(eve), Access::manage())
        ]
    );

    // All the following operations are appended into the group operation graph into a branch
    // concurrent to all the previous group changes. This means they should be validated against
    // state which does not include those changes (even though they are the "current" state).

    // AlreadyAdded (bob)
    let op = TestOperation {
        id: 1,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(bob),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_ii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::AlreadyAdded(GroupMember::Individual('B'))
        ))
    ));

    // Remove claire
    let op = TestOperation {
        id: 2,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(claire),
            },
        },
    };
    let y_iii = Group::process(y_ii.clone(), &op).unwrap();

    // Refer to only the newly published operation in previous so as to remain in the concurrent branch.
    let previous = vec![op.id];

    // AlreadyRemoved (claire)
    let op = TestOperation {
        id: 3,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(claire),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::AlreadyRemoved(GroupMember::Individual('C'))
        ))
    ));

    // InsufficientAccess (bob tries to add dave)
    let op = TestOperation {
        id: 4,
        author: bob,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iii.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::InsufficientAccess(GroupMember::Individual('B'))
        ))
    ));

    // Remove bob
    let op = TestOperation {
        id: 5,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(bob),
            },
        },
    };
    let y_iv = Group::process(y_iii.clone(), &op).unwrap();

    // Refer to only the newly published operation in previous so as to remain in the concurrent branch.
    let previous = vec![op.id];

    // InactiveActor (bob tries to add dave)
    let op = TestOperation {
        id: 6,
        author: bob,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iv.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::InactiveActor(GroupMember::Individual('B'))
        ))
    ));

    // InactiveMember (claire promoted)
    let op = TestOperation {
        id: 7,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(claire),
                access: Access::write(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iv.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::InactiveMember(GroupMember::Individual('C'))
        ))
    ));

    // UnrecognisedActor (eve tries to add dave)
    let op = TestOperation {
        id: 8,
        author: eve,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(dave),
                access: Access::read(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iv.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::UnrecognisedActor(GroupMember::Individual('E'))
        ))
    ));

    // UnrecognisedMember (alice promotes eve)
    let op = TestOperation {
        id: 9,
        author: alice,
        dependencies: previous.clone(),
        previous: previous.clone(),
        payload: GroupControlMessage::GroupAction {
            group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(eve),
                access: Access::write(),
            },
        },
    };
    assert!(matches!(
        Group::process(y_iv.clone(), &op),
        Err(GroupError::StateChangeError(
            _,
            GroupMembershipError::UnrecognisedMember(GroupMember::Individual('E'))
        ))
    ));
}
