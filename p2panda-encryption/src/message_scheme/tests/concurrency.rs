// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::crypto::Rng;
use crate::message_scheme::dcgka::{Dcgka, DirectMessageType, ProcessInput};
use crate::message_scheme::test_utils::dcgka::{
    ExpectedMembers, assert_direct_message, assert_members_view, init_dcgka_state,
};
use crate::test_utils::MessageId;

// From DCGKA paper (6.2.5):
//
// "In particular, care is required when an add operation occurs concurrently with an update,
// remove, or another add operation."
//
// "We want all intended recipients to learn every update secret, since otherwise some users would
// not be able to decrypt some messages, despite being a group member. For example, consider a
// group with members {A, B, C}, and say A performs an update while concurrently C adds D to the
// group. When A distributes a new seed secret through 2SM-encrypted direct messages, D will not be
// a recipient of one of those direct messages, since A did not know about D's addition at the time
// of sending."
//
// "D will therefore execute lines 6–7 of process-seed, and it cannot derive any of the member
// secrets for this update. When B updates its KDF ratchet using A's seed secret, it will compute
// an update secret that D does not know, and D will not be able to decrypt B's subsequent
// application messages."
//
// "In this example, B may receive the add and the update in either order. If B processes A's
// update first, the seed secret from A is already incorporated into B's ratchet state at time time
// of adding D; since B sends this ratchet state to D along with its "add-ack" message, no further
// action is needed. On the other hand, if B processes the addition of D first, then when B
// subsequently processes A's update, B must take the member secret it derives from A's seed secret
// and forward it to D, so that D can compute B's update secret for A's update."
#[test]
fn concurrent_operation() {
    let rng = Rng::from_seed([1; 32]);

    let alice = 0;
    let bob = 1;
    let charlie = 2;
    let dahlia = 3;
    let emil = 4;

    // Generate key material for all members, register pre-keys and initialise DCGKA states.
    let [
        alice_dcgka,
        bob_dcgka,
        charlie_dcgka,
        dahlia_dcgka,
        _emil_dcgka,
    ] = init_dcgka_state([alice, bob, charlie, dahlia, emil], &rng);

    // Alice creates a group with Bob and Charlie (similar to the paper's example).
    let (alice_dcgka, alice_output) = {
        let (alice_dcgka_pre, alice_pre) =
            Dcgka::create(alice_dcgka, vec![alice, bob, charlie], &rng).unwrap();
        Dcgka::process_local(
            alice_dcgka_pre,
            MessageId {
                sender: alice,
                seq: 0,
            },
            alice_pre,
            &rng,
        )
        .unwrap()
    };

    // Bob and Charlie process the "create" message from Alice.
    let (bob_dcgka, _bob_output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 0,
            },
            sender: alice,
            direct_message: Some(assert_direct_message(&alice_output.direct_messages, bob)),
            control_message: alice_output.control_message.clone(),
        },
        &rng,
    )
    .unwrap();

    let (charlie_dcgka, _charlie_output) = Dcgka::process_remote(
        charlie_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 0,
            },
            sender: alice,
            direct_message: Some(assert_direct_message(
                &alice_output.direct_messages,
                charlie,
            )),
            control_message: alice_output.control_message,
        },
        &rng,
    )
    .unwrap();

    // Everybody should consider each other part of the group.
    for dcgka in [&alice_dcgka, &bob_dcgka, &charlie_dcgka] {
        assert_members_view(
            dcgka,
            &[ExpectedMembers {
                viewer: &[alice, bob, charlie],
                expected: &[alice, bob, charlie],
            }],
        );
    }

    // Charlie adds Dahlia to the group (similar to paper's example).
    let (charlie_dcgka, charlie_output) = {
        let (charlie_dcgka_pre, charlie_pre) = Dcgka::add(charlie_dcgka, dahlia, &rng).unwrap();
        Dcgka::process_local(
            charlie_dcgka_pre,
            MessageId {
                sender: charlie,
                seq: 1,
            },
            charlie_pre,
            &rng,
        )
        .unwrap()
    };

    for dcgka in [&alice_dcgka, &bob_dcgka] {
        assert_members_view(
            dcgka,
            &[ExpectedMembers {
                viewer: &[alice, bob, charlie],
                expected: &[alice, bob, charlie],
            }],
        );
    }

    assert_members_view(
        &charlie_dcgka,
        &[ExpectedMembers {
            viewer: &[charlie, dahlia],
            expected: &[alice, bob, charlie, dahlia],
        }],
    );

    // Bob processes Dahlia's addition by Charlie.
    let (bob_dcgka, bob_output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: charlie,
                seq: 1,
            },
            sender: charlie,
            direct_message: None,
            control_message: charlie_output.control_message.clone(),
        },
        &rng,
    )
    .unwrap();

    assert_eq!(bob_output.direct_messages.len(), 1);
    assert_eq!(
        bob_output.direct_messages.first().unwrap().recipient,
        dahlia
    );

    assert_members_view(
        &bob_dcgka,
        &[
            ExpectedMembers {
                viewer: &[bob, charlie, dahlia],
                expected: &[alice, bob, charlie, dahlia],
            },
            ExpectedMembers {
                viewer: &[alice],
                expected: &[alice, bob, charlie],
            },
        ],
    );

    // Dahlia processes their addition by Charlie.
    let (dahlia_dcgka, _dahlia_output) = Dcgka::process_remote(
        dahlia_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: charlie,
                seq: 1,
            },
            sender: charlie,
            direct_message: Some(assert_direct_message(
                &charlie_output.direct_messages,
                dahlia,
            )),
            control_message: charlie_output.control_message,
        },
        &rng,
    )
    .unwrap();

    assert_members_view(
        &dahlia_dcgka,
        &[
            ExpectedMembers {
                viewer: &[charlie, dahlia],
                expected: &[alice, bob, charlie, dahlia],
            },
            ExpectedMembers {
                viewer: &[alice, bob],
                expected: &[alice, bob, charlie],
            },
        ],
    );

    // Dahlia processes the "ack" of Bob adding them and the direct message.
    let (dahlia_dcgka, _dahlia_output) = Dcgka::process_remote(
        dahlia_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: bob,
                seq: 1,
            },
            sender: bob,
            direct_message: Some(assert_direct_message(&bob_output.direct_messages, dahlia)),
            control_message: bob_output.control_message.as_ref().unwrap().clone(),
        },
        &rng,
    )
    .unwrap();

    for action in ["update", "remove", "another_add"] {
        // Concurrently Alice performs another group operation.
        let (_alice_dcgka, alice_output) = {
            let (alice_dcgka_pre, alice_pre) = match action {
                // Alice updates the group seed.
                "update" => Dcgka::update(alice_dcgka.clone(), &rng).unwrap(),
                // Alice concurrently removes Charlie who wanted to add Dahlia.
                "remove" => Dcgka::remove(alice_dcgka.clone(), charlie, &rng).unwrap(),
                // Alice adds Emil to the group.
                "another_add" => Dcgka::add(alice_dcgka.clone(), emil, &rng).unwrap(),
                _ => unreachable!(),
            };
            Dcgka::process_local(
                alice_dcgka_pre,
                MessageId {
                    sender: alice,
                    seq: 1,
                },
                alice_pre,
                &rng,
            )
            .unwrap()
        };

        // Bob processes Alice's operation _after_ they accepted Charlie's add of Dahlia.
        let (_bob_dcgka, bob_output) = Dcgka::process_remote(
            bob_dcgka.clone(),
            ProcessInput {
                seq: MessageId {
                    sender: alice,
                    seq: 1,
                },
                sender: alice,
                control_message: alice_output.control_message.clone(),
                direct_message: {
                    if action == "another_add" {
                        None
                    } else if action == "update" || action == "remove" {
                        Some(assert_direct_message(&alice_output.direct_messages, bob))
                    } else {
                        unreachable!()
                    }
                },
            },
            &rng,
        )
        .unwrap();

        if action == "remove" {
            // Bob processed the "remove" already, so they will not forward anything to Dahlia
            // anymore.
            assert_eq!(bob_output.direct_messages.len(), 0);
        } else if action == "another_add" {
            // Bob forwards a direct message to Emil as part of the regular "add" procedure.
            assert_eq!(bob_output.direct_messages.len(), 1);
            assert_eq!(bob_output.direct_messages.first().unwrap().recipient, emil);
            assert_eq!(
                bob_output.direct_messages.first().unwrap().message_type(),
                DirectMessageType::Forward
            );
        } else if action == "update" {
            // Bob forwards a direct message to Dahlia to account for the concurrent group update
            // of Alice.
            assert_eq!(bob_output.direct_messages.len(), 1);
            assert_eq!(
                bob_output.direct_messages.first().unwrap().recipient,
                dahlia
            );
            assert_eq!(
                bob_output.direct_messages.first().unwrap().message_type(),
                DirectMessageType::Forward
            );
        }

        // Dahlia processes the concurrent operation of Alice.
        let (dahlia_dcgka, dahlia_output) = Dcgka::process_remote(
            dahlia_dcgka.clone(),
            ProcessInput {
                seq: MessageId {
                    sender: alice,
                    seq: 1,
                },
                sender: alice,
                control_message: alice_output.control_message,
                // We don't expect any direct messages to Dahlia from Alice as Alice didn't know
                // about them when they performed the group operation.
                direct_message: None,
            },
            &rng,
        )
        .unwrap();

        if action == "another_add" {
            // During an add Dahlia could already derive the temporary update secret.
            assert!(dahlia_output.me_update_secret.is_some());
        } else {
            // .. otherwise they can't do much yet.
            assert!(dahlia_output.me_update_secret.is_none());
        }
        assert!(dahlia_output.sender_update_secret.is_none());

        // Dahlia processes the direct "forward" message and "ack" of Bob.
        if action == "update" {
            let (_dahlia_dcgka, dahlia_output) = Dcgka::process_remote(
                dahlia_dcgka,
                ProcessInput {
                    seq: MessageId {
                        sender: bob,
                        seq: 2,
                    },
                    sender: bob,
                    direct_message: Some(assert_direct_message(
                        &bob_output.direct_messages,
                        dahlia,
                    )),
                    control_message: bob_output.control_message.as_ref().unwrap().clone(),
                },
                &rng,
            )
            .unwrap();

            // Dahlia's update secret matches the one of Bob after handling all concurrent actions.
            assert_eq!(
                dahlia_output.sender_update_secret.unwrap(),
                bob_output.me_update_secret.unwrap()
            );
        }
    }
}

// From DCGKA paper (6.2.5):
//
// "Another scenario that needs to be handled is when two users are concurrently added to the
// group."
//
// "For example, in a group consisting initially of {A, B}, say A adds C to the group, while
// concurrently B adds D. User C first processes its own addition and welcome message, and then
// processes B's addition of D. However, since C was not a group member at the time B sent its
// "add" message, C does not yet have B's ratchet state, so C cannot derive an update secret for
// B's "add" message. The condition on line 5 of process-add is false and so C does not derive an
// update secret on lines 6–8. When B finds out about the fact that A has added C, B sends C its
// ratchet state as usual (line 12 of process-add), so C can initialize its copy of B's ratchet as
// before (lines 4–5 of process-add-ack)."
//
// "Similarly, when D finds out about the fact that A has added C, D sends its ratchet state to C
// along with the "add-ack" message. The existing logic therefore handles the concurrent additions:
// after all acks have been delivered, C and D have both initialized their copies of all four
// ratchets, and so they are able to decrypt application messages that any group member sent after
// processing their addition."
#[test]
fn concurrent_adds() {
    let rng = Rng::from_seed([1; 32]);

    let alice = 0;
    let bob = 1;
    let charlie = 2;
    let dahlia = 3;

    // Generate key material for all members, register pre-keys and initialise DCGKA states.
    let [alice_dcgka, bob_dcgka, charlie_dcgka, dahlia_dcgka] =
        init_dcgka_state([alice, bob, charlie, dahlia], &rng);

    // Alice creates a group with Bob (similar to the paper's example).
    let (alice_dcgka, alice_output) = {
        let (alice_dcgka_pre, alice_pre) =
            Dcgka::create(alice_dcgka, vec![alice, bob], &rng).unwrap();
        Dcgka::process_local(
            alice_dcgka_pre,
            MessageId {
                sender: alice,
                seq: 0,
            },
            alice_pre,
            &rng,
        )
        .unwrap()
    };

    // Bob processes the "create" message from Alice.
    let (bob_dcgka, bob_output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 0,
            },
            sender: alice,
            direct_message: Some(assert_direct_message(&alice_output.direct_messages, bob)),
            control_message: alice_output.control_message,
        },
        &rng,
    )
    .unwrap();

    // Bob learned about Alice's update secret.
    assert_eq!(
        &alice_output.me_update_secret.unwrap(),
        bob_output.sender_update_secret.as_ref().unwrap()
    );

    // Alice processing the "ack" of Bob.
    let (alice_dcgka, alice_output) = Dcgka::process_remote(
        alice_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: bob,
                seq: 0,
            },
            sender: bob,
            direct_message: None,
            control_message: bob_output.control_message.unwrap(),
        },
        &rng,
    )
    .unwrap();

    // Alice learned about Bob's update secret.
    assert_eq!(
        alice_output.sender_update_secret.unwrap(),
        bob_output.me_update_secret.unwrap()
    );

    // Everybody should consider each other part of the group.
    for dcgka in [&alice_dcgka, &bob_dcgka] {
        assert_members_view(
            dcgka,
            &[ExpectedMembers {
                viewer: &[alice, bob],
                expected: &[alice, bob],
            }],
        );
    }

    // Alice adds Charlie to the group (as example in paper).
    let (_alice_dcgka, alice_output) = {
        let (alice_dcgka_pre, alice_pre) = Dcgka::add(alice_dcgka, charlie, &rng).unwrap();
        Dcgka::process_local(
            alice_dcgka_pre,
            MessageId {
                sender: alice,
                seq: 1,
            },
            alice_pre,
            &rng,
        )
        .unwrap()
    };

    // Bob concurrently adds Dahlia to the group (as example in paper).
    let (bob_dcgka, bob_output) = {
        let (bob_dcgka_pre, bob_pre) = Dcgka::add(bob_dcgka, dahlia, &rng).unwrap();
        Dcgka::process_local(
            bob_dcgka_pre,
            MessageId {
                sender: bob,
                seq: 1,
            },
            bob_pre,
            &rng,
        )
        .unwrap()
    };

    // Charlie processes their own addition by Alice.
    let (charlie_dcgka, _charlie_output) = Dcgka::process_remote(
        charlie_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 1,
            },
            sender: alice,
            direct_message: Some(assert_direct_message(
                &alice_output.direct_messages,
                charlie,
            )),
            control_message: alice_output.control_message.clone(),
        },
        &rng,
    )
    .unwrap();

    // Dahlia processes their own add by Bob.
    let (dahlia_dcgka, _dahlia_output) = Dcgka::process_remote(
        dahlia_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: bob,
                seq: 1,
            },
            sender: bob,
            direct_message: Some(assert_direct_message(&bob_output.direct_messages, dahlia)),
            control_message: bob_output.control_message,
        },
        &rng,
    )
    .unwrap();

    // Dahlia processes Alice's addition of Charlie.
    let (dahlia_dcgka, _dahlia_output) = Dcgka::process_remote(
        dahlia_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 1,
            },
            sender: alice,
            direct_message: None,
            control_message: alice_output.control_message.clone(),
        },
        &rng,
    )
    .unwrap();

    assert_members_view(
        &dahlia_dcgka,
        &[ExpectedMembers {
            viewer: &[dahlia],
            expected: &[alice, bob, charlie, dahlia],
        }],
    );

    // Bob processes Charlie's "add" by Alice and finally learns that this happenend concurrently
    // to their own add of Dahlia.
    let (bob_dcgka, bob_output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 1,
            },
            sender: alice,
            direct_message: None,
            control_message: alice_output.control_message,
        },
        &rng,
    )
    .unwrap();

    assert_members_view(
        &bob_dcgka,
        &[
            ExpectedMembers {
                viewer: &[bob],
                expected: &[alice, bob, charlie, dahlia],
            },
            ExpectedMembers {
                viewer: &[charlie],
                expected: &[alice, bob, charlie],
            },
        ],
    );

    // Bob prepares a direct "forward" message to Charlie due to the concurrent operations.
    assert_eq!(bob_output.direct_messages.len(), 1);
    assert_eq!(
        bob_output.direct_messages.first().unwrap().recipient,
        charlie
    );
    assert_eq!(
        bob_output.direct_messages.first().unwrap().message_type(),
        DirectMessageType::Forward
    );

    // Charlie processes the "add-ack" of Bob and handles the direct "forward" message.
    let (_charlie_dcgka, charlie_output) = Dcgka::process_remote(
        charlie_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: bob,
                seq: 1,
            },
            sender: bob,
            direct_message: Some(assert_direct_message(&bob_output.direct_messages, charlie)),
            control_message: bob_output.control_message.unwrap(),
        },
        &rng,
    )
    .unwrap();

    assert_eq!(
        bob_output.me_update_secret.unwrap(),
        charlie_output.sender_update_secret.unwrap()
    );
}
