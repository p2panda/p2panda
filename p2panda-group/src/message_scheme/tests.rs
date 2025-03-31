// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::crypto::x25519::SecretKey;
use crate::message_scheme::acked_dgm::test_utils::AckedTestDGM;
use crate::message_scheme::dcgka::{AckMessage, AddAckMessage, ControlMessage};
use crate::message_scheme::{Dcgka, DcgkaState, DirectMessageType, ProcessInput, ProcessMessage};
use crate::traits::{AckedGroupMembership, PreKeyManager};
use crate::{KeyManager, KeyRegistry, Lifetime, Rng};

type MemberId = usize;

type MessageId = usize;

type TestDcgkaState = DcgkaState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    AckedTestDGM<MemberId, MessageId>,
    KeyManager,
>;

/// Helper method returning initialised DCGKA state for each member of a test group.
///
/// The method will automatically generate all required one-time pre-key bundles from each member
/// and register them for each other.
fn init_dcgka_state<const N: usize>(member_ids: [MemberId; N], rng: &Rng) -> [TestDcgkaState; N] {
    let mut key_bundles = HashMap::new();
    let mut key_managers = HashMap::new();

    // Generate a pre-key bundle for each other member of the group.
    for id in member_ids {
        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let mut manager = KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap();

        let mut bundle_list = Vec::with_capacity(member_ids.len());
        for _ in member_ids {
            let (manager_i, key_bundle) =
                KeyManager::generate_onetime_bundle(manager, &rng).unwrap();
            bundle_list.push(key_bundle);
            manager = manager_i;
        }

        key_bundles.insert(id, bundle_list);
        key_managers.insert(id, manager);
    }

    // Register each other's pre-key bundles and initialise DCGKA state.
    let mut result = Vec::with_capacity(member_ids.len());
    for id in member_ids {
        let dgm = AckedTestDGM::init(id);
        let registry = {
            let mut state = KeyRegistry::init();
            for bundle_id in member_ids {
                let bundle = key_bundles.get_mut(&bundle_id).unwrap().pop().unwrap();
                let state_i = KeyRegistry::add_onetime_bundle(state, bundle_id, bundle);
                state = state_i;
            }
            state
        };
        let manager = key_managers.remove(&id).unwrap();
        let dcgka: TestDcgkaState = Dcgka::init(id, manager, registry, dgm);
        result.push(dcgka);
    }

    result.try_into().unwrap()
}

#[test]
fn it_works() {
    let rng = Rng::from_seed([1; 32]);

    let alice = 0;
    let bob = 1;
    let charlie = 2;

    // Generate key material for all members, register pre-keys and initialise DCGKA states.
    let [alice_dcgka, bob_dcgka, charlie_dcgka] = init_dcgka_state([alice, bob, charlie], &rng);

    // =================================
    // 1. Alice creates a group with Bob
    // =================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0) <--
    // [ ] Bob's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [ ] Alice's Rachet (0)
    // [ ] Bob's Ratchet (0)

    let (alice_dcgka_0, alice_0) = {
        let (alice_dcgka_pre, alice_pre) =
            Dcgka::create(alice_dcgka, vec![alice, bob], &rng).unwrap();

        let (alice_dcgka_0, alice_0) =
            Dcgka::process_local(alice_dcgka_pre, 0, alice_pre, &rng).unwrap();

        // a) Alice broadcasts a "Create" control message (seq_num = 0) to everyone.
        assert!(matches!(alice_0.control_message, ControlMessage::Create(_)));

        // b) Alice sends a direct 2SM message to each other member of the group (one for Bob).
        assert_eq!(alice_0.direct_messages.len(), 1);
        assert_eq!(
            alice_0.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::TwoParty
        );
        assert_eq!(alice_0.direct_messages.get(0).unwrap().recipient, bob);

        // c) Alice establishes the update secret for their own message ratchet.
        assert!(alice_0.me_update_secret.is_some());

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&alice_dcgka_0.dgm, &alice).unwrap(),
            HashSet::from([alice, bob])
        );

        (alice_dcgka_0, alice_0)
    };

    // ===============================
    // 2. Bob processes Alice's Create
    // ===============================
    //
    // Bob processes the "invitation" and establishes the same state for Alice's ratchet. Bob
    // "acks" the "create" of Alice and returns it as a control message.
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Bob's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0) <--
    // [x] Bob's Ratchet (0) <--

    let (bob_dcgka_0, bob_0) = {
        let ControlMessage::Create(alice_0_create) = alice_0.control_message else {
            panic!("expected create message");
        };
        let alice_0_direct_message = alice_0
            .direct_messages
            .get(0)
            .expect("direct message")
            .to_owned();

        let (bob_dcgka_0, bob_0) = Dcgka::process_remote(
            bob_dcgka,
            ProcessInput {
                seq: 0,
                sender: alice,
                message: ProcessMessage::Create(alice_0_create, alice_0_direct_message),
            },
            &rng,
        )
        .unwrap();

        // a) Bob broadcasts an "Ack" control message (seq_num = 0) to everyone, no direct
        // messages.
        assert!(bob_0.control_message.is_some());
        assert!(matches!(
            bob_0.control_message.as_ref().unwrap(),
            ControlMessage::Ack(_)
        ));
        assert!(bob_0.direct_messages.is_empty());

        // b) Bob establishes the update secret for their own message ratchet.
        assert!(bob_0.me_update_secret.is_some());

        // c) Bob establishes the update secret for Alice's message ratchet.
        assert!(bob_0.sender_update_secret.is_some());
        assert_eq!(
            bob_0.sender_update_secret.as_ref().unwrap().as_bytes(),
            alice_0.me_update_secret.as_ref().unwrap().as_bytes()
        );

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&bob_dcgka_0.dgm, &bob).unwrap(),
            HashSet::from([alice, bob])
        );
        assert_eq!(
            AckedTestDGM::members_view(&bob_dcgka_0.dgm, &alice).unwrap(),
            HashSet::from([alice, bob])
        );

        (bob_dcgka_0, bob_0)
    };

    // ============================
    // 3. Alice processes Bob's Ack
    // ============================
    //
    // Alice processes the ack of Bob and establishes the same state for Bob's ratchet.
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Bob's Ratchet (0) <--
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Bob's Ratchet (0)

    let (alice_dcgka_1, _alice_1) = {
        let bob_0_control = bob_0.control_message.expect("control message");
        let ControlMessage::Ack(bob_0_ack) = bob_0_control else {
            panic!("expected ack message");
        };
        let AckMessage {
            ack_sender,
            ack_seq,
        } = bob_0_ack;
        assert_eq!(ack_sender, alice);
        assert_eq!(ack_seq, 0);

        let (alice_dcgka_1, alice_1) = Dcgka::process_remote(
            alice_dcgka_0,
            ProcessInput {
                seq: 0,
                sender: bob,
                message: ProcessMessage::Ack(bob_0_ack, None),
            },
            &rng,
        )
        .unwrap();

        // a) No control messages or direct messages.
        assert!(alice_1.control_message.is_none());
        assert!(alice_1.direct_messages.is_empty());

        // b) No new update secret for Alice's message ratchet.
        assert!(alice_1.me_update_secret.is_none());

        // c) Alice establishes the update secret for Bob's message ratchet.
        assert!(alice_1.sender_update_secret.is_some());
        assert_eq!(
            alice_1.sender_update_secret.as_ref().unwrap().as_bytes(),
            bob_0.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (alice_dcgka_1, alice_1)
    };

    // ================================
    // 4. Bob adds Charlie to the group
    // ================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [ ] Bob's Ratchet (1)
    // [ ] Charlie's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1) <--
    // [ ] Charlie's Ratchet (0)
    //
    // Charlie's perspective:
    //
    // [ ] Alice's Rachet (1)
    // [ ] Bob's Ratchet (1)
    // [ ] Charlie's Ratchet (0)

    let (bob_dcgka_1, bob_1) = {
        let (bob_dcgka_pre, bob_pre) = Dcgka::add(bob_dcgka_0, charlie, &rng).unwrap();

        let (bob_dcgka_1, bob_1) = Dcgka::process_local(bob_dcgka_pre, 1, bob_pre, &rng).unwrap();

        // a) Bob broadcasts an "Add" (seq_num = 1) control message to everyone.
        assert!(matches!(bob_1.control_message, ControlMessage::Add(_)));

        // b) Bob sends a welcome message to Charlie.
        assert_eq!(bob_1.direct_messages.len(), 1);
        assert_eq!(
            bob_1.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::Welcome
        );
        assert_eq!(bob_1.direct_messages.get(0).unwrap().recipient, charlie);

        // c) Bob establishes a new update secret for their own message ratchet.
        assert!(bob_1.me_update_secret.is_some());
        assert_ne!(
            bob_0.me_update_secret.as_ref().unwrap().as_bytes(),
            bob_1.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&bob_dcgka_1.dgm, &bob).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        (bob_dcgka_1, bob_1)
    };

    // ========================================
    // 5. Charlie processes Bob's Add & Welcome
    // ========================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [ ] Bob's Ratchet (1)
    // [ ] Charlie's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [ ] Charlie's Ratchet (0)
    //
    // Charlie's perspective:
    //
    // [ ] Alice's Rachet (1)
    // [x] Bob's Ratchet (1) <--
    // [x] Charlie's Ratchet (0) <--

    let (charlie_dcgka_0, charlie_0) = {
        let bob_1_control = bob_1.control_message.clone();
        let ControlMessage::Add(bob_1_add) = bob_1_control else {
            panic!("expected add message");
        };
        let bob_1_direct_message = bob_1
            .direct_messages
            .first()
            .expect("direct message")
            .to_owned();

        let (charlie_dcgka_0, charlie_0) = Dcgka::process_remote(
            charlie_dcgka,
            ProcessInput {
                seq: 1,
                sender: bob,
                message: ProcessMessage::Add(bob_1_add, Some(bob_1_direct_message)),
            },
            &rng,
        )
        .unwrap();

        // a) Charlie broadcasts an "Ack" (seq_num = 0) control message to everyone, no direct
        // messages.
        assert!(charlie_0.control_message.is_some());
        assert!(matches!(
            charlie_0.control_message.as_ref().unwrap(),
            ControlMessage::Ack(_)
        ));
        assert!(charlie_0.direct_messages.is_empty());

        // b) Charlie establishes a new update secret for their own message ratchet.
        assert!(charlie_0.me_update_secret.is_some());

        // c) Charlie establishes the update secret for Bob's message ratchet.
        assert!(charlie_0.sender_update_secret.is_some());
        assert_eq!(
            charlie_0.sender_update_secret.as_ref().unwrap().as_bytes(),
            bob_1.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&charlie_dcgka_0.dgm, &charlie).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        (charlie_dcgka_0, charlie_0)
    };

    // =======================================
    // 6. Alice processes Bob's Add of Charlie
    // =======================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Alice's Ratchet (1) <--
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1) <--
    // [ ] Charlie's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [ ] Charlie's Ratchet (0)
    //
    // Charlie's perspective:
    //
    // [ ] Alice's Rachet (1)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)

    let (alice_dcgka_2, alice_2) = {
        let bob_1_control = bob_1.control_message.clone();
        let ControlMessage::Add(bob_1_add) = bob_1_control else {
            panic!("expected add message");
        };

        let (alice_dcgka_2, alice_2) = Dcgka::process_remote(
            alice_dcgka_1,
            ProcessInput {
                seq: 1,
                sender: bob,
                message: ProcessMessage::Add(bob_1_add, None),
            },
            &rng,
        )
        .unwrap();

        // a) Alice broadcasts an "AddAck" (seq_num = 1) control message to everyone.
        assert!(alice_2.control_message.is_some());
        assert!(matches!(
            alice_2.control_message.as_ref().unwrap(),
            ControlMessage::AddAck(_)
        ));

        // b) Alice forwards a direct message to Charlie. It is required so Charlie can decrypt
        // subsequent messages of Alice.
        assert_eq!(alice_2.direct_messages.len(), 1);
        assert_eq!(alice_2.direct_messages.get(0).unwrap().recipient, charlie);
        assert_eq!(
            alice_2.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::Forward
        );

        // c) Alice establishes a new update secret for their own message ratchet.
        assert!(alice_2.me_update_secret.is_some());

        // d) Alice establishes the update secret for Bob's message ratchet.
        assert!(alice_2.sender_update_secret.is_some());
        assert_eq!(
            alice_2.sender_update_secret.as_ref().unwrap().as_bytes(),
            bob_1.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&alice_dcgka_2.dgm, &alice).unwrap(),
            HashSet::from([alice, bob, charlie])
        );

        (alice_dcgka_2, alice_2)
    };

    // ==============================
    // 7. Bob processes Charlie's Ack
    // ==============================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [ ] Charlie's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0) <--
    //
    // Charlie's perspective:
    //
    // [ ] Alice's Rachet (1)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)

    let (bob_dcgka_2, _bob_2) = {
        let charlie_0_control = charlie_0
            .control_message
            .as_ref()
            .expect("control message")
            .clone();
        let ControlMessage::Ack(charlie_0_ack) = charlie_0_control else {
            panic!("expected ack message");
        };
        let AckMessage {
            ack_sender,
            ack_seq,
        } = charlie_0_ack;
        assert_eq!(ack_sender, bob);
        assert_eq!(ack_seq, 1);

        let (bob_dcgka_2, bob_2) = Dcgka::process_remote(
            bob_dcgka_1,
            ProcessInput {
                seq: 0,
                sender: charlie,
                message: ProcessMessage::Ack(charlie_0_ack, None),
            },
            &rng,
        )
        .unwrap();

        // a) No control messages and no direct messages.
        assert!(bob_2.control_message.is_none());
        assert!(bob_2.direct_messages.is_empty());

        // b) No new update secret for Bob's own message ratchet.
        assert!(bob_2.me_update_secret.is_none());

        // c) Bob establishes update secret for Charlie's message ratchet.
        assert!(bob_2.sender_update_secret.is_some());
        assert_eq!(
            bob_2.sender_update_secret.as_ref().unwrap().as_bytes(),
            charlie_0.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (bob_dcgka_2, bob_2)
    };

    // ================================
    // 8. Alice processes Charlie's Ack
    // ================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0) <--
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)
    //
    // Charlie's perspective:
    //
    // [ ] Alice's Rachet (1)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)

    let (_alice_dcgka_3, _alice_3) = {
        let charlie_0_control = charlie_0.control_message.expect("control message");
        let ControlMessage::Ack(charlie_0_ack) = charlie_0_control else {
            panic!("expected ack message");
        };
        let AckMessage {
            ack_sender,
            ack_seq,
        } = charlie_0_ack;
        assert_eq!(ack_sender, bob);
        assert_eq!(ack_seq, 1);

        let (alice_dcgka_3, alice_3) = Dcgka::process_remote(
            alice_dcgka_2,
            ProcessInput {
                seq: 0,
                sender: charlie,
                message: ProcessMessage::Ack(charlie_0_ack, None),
            },
            &rng,
        )
        .unwrap();

        // a) No control messages and no direct messages.
        assert!(alice_3.control_message.is_none());
        assert!(alice_3.direct_messages.is_empty());

        // b) No new update secret for Alice's own message ratchet.
        assert!(alice_3.me_update_secret.is_none());

        // c) Alice establishes the message ratchet for Charlie.
        assert!(alice_3.sender_update_secret.is_some());
        assert_eq!(
            alice_3.sender_update_secret.as_ref().unwrap().as_bytes(),
            charlie_0.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (alice_dcgka_3, alice_3)
    };

    // ===================================
    // 9. Charlie processes Alice's AddAck
    // ===================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [ ] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)
    //
    // Charlie's perspective:
    //
    // [x] Alice's Rachet (1)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)

    let (charlie_dcgka_1, _charlie_1) = {
        let alice_2_control = alice_2
            .control_message
            .as_ref()
            .expect("control message")
            .clone();
        let ControlMessage::AddAck(alice_2_add_ack) = alice_2_control else {
            panic!("expected add-ack message");
        };
        let AddAckMessage {
            ack_sender,
            ack_seq,
        } = alice_2_add_ack;
        assert_eq!(ack_sender, bob);
        assert_eq!(ack_seq, 1);
        let alice_2_direct_message = alice_2
            .direct_messages
            .first()
            .expect("direct message")
            .to_owned();

        let (charlie_dcgka_1, charlie_1) = Dcgka::process_remote(
            charlie_dcgka_0,
            ProcessInput {
                seq: 1,
                sender: alice,
                message: ProcessMessage::AddAck(alice_2_add_ack, Some(alice_2_direct_message)),
            },
            &rng,
        )
        .unwrap();

        // a) No control messages and no direct messages.
        assert!(charlie_1.control_message.is_none());
        assert!(charlie_1.direct_messages.is_empty());

        // b) No new update secret for Charlie's own message ratchet.
        assert!(charlie_1.me_update_secret.is_none());

        // c) Charlie establishes the message ratchet for Alice.
        assert!(charlie_1.sender_update_secret.is_some());
        assert_eq!(
            charlie_1.sender_update_secret.as_ref().unwrap().as_bytes(),
            alice_2.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (charlie_dcgka_1, charlie_1)
    };

    // ================================
    // 10. Bob processes Alice's AddAck
    // ================================
    //
    // Alice's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Alice's Ratchet (1)
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)
    //
    // Bob's perspective:
    //
    // [x] Alice's Rachet (0)
    // [x] Alice's Ratchet (1) <--
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)
    //
    // Charlie's perspective:
    //
    // [x] Alice's Rachet (1)
    // [x] Bob's Ratchet (1)
    // [x] Charlie's Ratchet (0)

    let (bob_dcgka_3, _bob_3) = {
        let alice_2_control = alice_2.control_message.expect("control message");
        let ControlMessage::AddAck(alice_2_add_ack) = alice_2_control else {
            panic!("expected add-ack message");
        };

        let (bob_dcgka_3, bob_3) = Dcgka::process_remote(
            bob_dcgka_2,
            ProcessInput {
                seq: 1,
                sender: alice,
                message: ProcessMessage::AddAck(alice_2_add_ack, None),
            },
            &rng,
        )
        .unwrap();

        // a) No control messages and no direct messages.
        assert!(bob_3.control_message.is_none());
        assert!(bob_3.direct_messages.is_empty());

        // b) No new update secret for Bob's own message ratchet.
        assert!(bob_3.me_update_secret.is_none());

        // c) Bob establishes the message ratchet for Alice.
        assert!(bob_3.sender_update_secret.is_some());
        assert_eq!(
            bob_3.sender_update_secret.as_ref().unwrap().as_bytes(),
            alice_2.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (bob_dcgka_3, bob_3)
    };

    // =========================
    // 11. Charlie removes Alice
    // =========================
    //
    // Bob's perspective:
    //
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [ ] Bob's Ratchet (2)
    // [x] Charlie's Ratchet (0)
    // [ ] Charlie's Ratchet (1)
    //
    // Charlie's perspective:
    //
    // [x] Bob's Ratchet (1)
    // [ ] Bob's Ratchet (2)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1) <--

    let (charlie_dcgka_2, charlie_2) = {
        let (charlie_dcgka_pre, charlie_pre) = Dcgka::remove(charlie_dcgka_1, alice, &rng).unwrap();

        let (charlie_dcgka_2, charlie_2) =
            Dcgka::process_local(charlie_dcgka_pre, 1, charlie_pre, &rng).unwrap();

        // a) Charlie broadcasts a "Remove" control message (seq_num = 1) to everyone.
        assert!(matches!(
            charlie_2.control_message,
            ControlMessage::Remove(_)
        ));

        // b) Charlie sends a direct 2SM message to each other member of the group (one for Bob).
        assert_eq!(charlie_2.direct_messages.len(), 1);
        assert_eq!(
            charlie_2.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::TwoParty
        );
        assert_eq!(charlie_2.direct_messages.get(0).unwrap().recipient, bob);

        // c) Charlie establishes a new update secret for their own message ratchet.
        assert!(charlie_2.me_update_secret.is_some());

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&charlie_dcgka_2.dgm, &charlie).unwrap(),
            HashSet::from([bob, charlie])
        );

        (charlie_dcgka_2, charlie_2)
    };

    // ===========================================
    // 12. Bob processes Charlie's Remove of Alice
    // ===========================================
    //
    // Bob's perspective:
    //
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2) <--
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1) <--
    //
    // Charlie's perspective:
    //
    // [x] Bob's Ratchet (1)
    // [ ] Bob's Ratchet (2)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)

    let (bob_dcgka_4, bob_4) = {
        let ControlMessage::Remove(charlie_2_remove) = charlie_2.control_message else {
            panic!("expected remove message");
        };
        let charlie_2_direct_message = charlie_2
            .direct_messages
            .first()
            .expect("direct message")
            .to_owned();

        let (bob_dcgka_4, bob_4) = Dcgka::process_remote(
            bob_dcgka_3,
            ProcessInput {
                seq: 1,
                sender: charlie,
                message: ProcessMessage::Remove(charlie_2_remove, charlie_2_direct_message),
            },
            &rng,
        )
        .unwrap();

        // a) Bob broadcasts an "Ack" control message for everyone (seq_num = 2), no direct
        // messages.
        assert!(bob_4.control_message.is_some());
        assert!(matches!(
            bob_4.control_message.as_ref().unwrap(),
            ControlMessage::Ack(_)
        ));
        assert!(bob_4.direct_messages.is_empty());

        // b) Bob establishes a new update secret for their own message ratchet.
        assert!(bob_4.me_update_secret.is_some());

        // c) Bob establishes the message ratchet for Charlie.
        assert!(bob_4.sender_update_secret.is_some());
        assert_eq!(
            bob_4.sender_update_secret.as_ref().unwrap().as_bytes(),
            charlie_2.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&bob_dcgka_4.dgm, &bob).unwrap(),
            HashSet::from([bob, charlie])
        );

        (bob_dcgka_4, bob_4)
    };

    // ===============================
    // 13. Charlie processes Bob's Ack
    // ===============================
    //
    // Bob's perspective:
    //
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    //
    // Charlie's perspective:
    //
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2) <--
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)

    let (charlie_dcgka_3, _charlie_3) = {
        let ControlMessage::Ack(bob_4_ack) = bob_4.control_message.unwrap() else {
            panic!("expected ack message");
        };
        let AckMessage {
            ack_sender,
            ack_seq,
        } = bob_4_ack;
        assert_eq!(ack_sender, charlie);
        assert_eq!(ack_seq, 1);

        let (charlie_dcgka_3, charlie_3) = Dcgka::process_remote(
            charlie_dcgka_2,
            ProcessInput {
                seq: 2,
                sender: bob,
                message: ProcessMessage::Ack(bob_4_ack, None),
            },
            &rng,
        )
        .unwrap();

        // a) No control or direct messages.
        assert!(charlie_3.control_message.is_none());
        assert!(charlie_3.direct_messages.is_empty());

        // b) No new update secret for Charlie.
        assert!(charlie_3.me_update_secret.is_none());

        // c) Charlie establishes the message ratchet for Bob.
        assert!(charlie_3.sender_update_secret.is_some());
        assert_eq!(
            charlie_3.sender_update_secret.as_ref().unwrap().as_bytes(),
            bob_4.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (charlie_dcgka_3, charlie_3)
    };

    // ===================================
    // 13. Bob updates the group's secrets
    // ===================================
    //
    // Bob's perspective:
    //
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [x] Bob's Ratchet (3) <--
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    // [ ] Charlie's Ratchet (2)
    //
    // Charlie's perspective:
    //
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [ ] Bob's Ratchet (3)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    // [ ] Charlie's Ratchet (2)

    let (bob_dcgka_5, bob_5) = {
        let (bob_dcgka_pre, bob_pre) = Dcgka::update(bob_dcgka_4, &rng).unwrap();

        let (bob_dcgka_5, bob_5) = Dcgka::process_local(bob_dcgka_pre, 3, bob_pre, &rng).unwrap();

        // a) Bob broadcasts an "Update" control message (seq_num = 3) to everyone.
        assert!(matches!(bob_5.control_message, ControlMessage::Update(_)));

        // b) Bob sends a direct 2SM message to each other member of the group (one for
        // Charlie).
        assert_eq!(bob_5.direct_messages.len(), 1);
        assert_eq!(
            bob_5.direct_messages.get(0).unwrap().message_type(),
            DirectMessageType::TwoParty
        );
        assert_eq!(bob_5.direct_messages.get(0).unwrap().recipient, charlie);

        // c) Bob establishes a new update secret for their own message ratchet.
        assert!(bob_5.me_update_secret.is_some());

        (bob_dcgka_5, bob_5)
    };

    // ==================================
    // 14. Charlie processes Bob's Update
    // ==================================
    //
    // Bob's perspective:
    //
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [x] Bob's Ratchet (3)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    // [ ] Charlie's Ratchet (2)
    //
    // Charlie's perspective:
    //
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [x] Bob's Ratchet (3) <--
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    // [x] Charlie's Ratchet (2) <--

    let (_charlie_dcgka_4, charlie_4) = {
        let ControlMessage::Update(bob_5_update) = bob_5.control_message else {
            panic!("expected update message");
        };
        let bob_5_direct_message = bob_5
            .direct_messages
            .first()
            .expect("direct message")
            .to_owned();

        let (charlie_dcgka_4, charlie_4) = Dcgka::process_remote(
            charlie_dcgka_3,
            ProcessInput {
                seq: 3,
                sender: bob,
                message: ProcessMessage::Update(bob_5_update, bob_5_direct_message),
            },
            &rng,
        )
        .unwrap();

        // a) Charlie broadcasts an "Ack" (seq_num = 2) to everyone.
        assert!(charlie_4.control_message.is_some());
        assert!(matches!(
            charlie_4.control_message.as_ref().unwrap(),
            ControlMessage::Ack(_)
        ));
        assert!(charlie_4.direct_messages.is_empty());

        // b) Charlie establishes a new update secret for their local message ratchet.
        assert!(charlie_4.me_update_secret.is_some());

        // c) Charlie establishes the message ratchet for Bob.
        assert!(charlie_4.sender_update_secret.is_some());
        assert_eq!(
            charlie_4.sender_update_secret.as_ref().unwrap().as_bytes(),
            bob_5.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        (charlie_dcgka_4, charlie_4)
    };

    // ===============================
    // 15. Bob processes Charlie's Ack
    // ===============================
    //
    // Bob's perspective:
    //
    // [x] Bob's Ratchet (0)
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [x] Bob's Ratchet (3)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    // [x] Charlie's Ratchet (2) <--
    //
    // Charlie's perspective:
    //
    // [x] Bob's Ratchet (1)
    // [x] Bob's Ratchet (2)
    // [x] Bob's Ratchet (3)
    // [x] Charlie's Ratchet (0)
    // [x] Charlie's Ratchet (1)
    // [x] Charlie's Ratchet (2)

    let (_bob_dcgka_6, _bob_6) = {
        let ControlMessage::Ack(charlie_4_ack) = charlie_4.control_message.unwrap() else {
            panic!("expected ack message");
        };

        let (bob_dcgka_6, bob_6) = Dcgka::process_remote(
            bob_dcgka_5,
            ProcessInput {
                seq: 2,
                sender: charlie,
                message: ProcessMessage::Ack(charlie_4_ack, None),
            },
            &rng,
        )
        .unwrap();

        // a) No control or direct messages.
        assert!(bob_6.control_message.is_none());
        assert!(bob_6.direct_messages.is_empty());

        // b) No new local update secret for Bob.
        assert!(bob_6.me_update_secret.is_none());

        // c) Bob establishes the message ratchet for Charlie.
        assert!(bob_6.sender_update_secret.is_some());
        assert_eq!(
            bob_6.sender_update_secret.as_ref().unwrap().as_bytes(),
            charlie_4.me_update_secret.as_ref().unwrap().as_bytes(),
        );

        // Check local state.
        assert_eq!(
            AckedTestDGM::members_view(&bob_dcgka_6.dgm, &bob).unwrap(),
            HashSet::from([bob, charlie])
        );

        (bob_dcgka_6, bob_6)
    };
}
