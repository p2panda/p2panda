// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Rng;
use crate::message_scheme::test_utils::{AssertableDcgka, init_dcgka_state};
use crate::message_scheme::{Dcgka, ProcessInput};

#[test]
fn group_operations() {
    let rng = Rng::from_seed([1; 32]);

    let alice = 0;
    let bob = 1;
    let charlie = 2;

    // Generate key material for all members, register pre-keys and initialise DCGKA states.
    let [alice_dcgka, bob_dcgka, charlie_dcgka] = init_dcgka_state([alice, bob, charlie], &rng);

    let mut test = AssertableDcgka::new();

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

    let (alice_dcgka_0, alice_0_seq_0) = {
        let (alice_dcgka_pre, alice_pre) =
            Dcgka::create(alice_dcgka, vec![alice, bob], &rng).unwrap();
        let seq = 0;
        let (alice_dcgka_0, alice_0) =
            Dcgka::process_local(alice_dcgka_pre, seq, alice_pre, &rng).unwrap();
        test.assert_create(&alice_dcgka_0, &alice_0, alice, &[alice, bob]);
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

    let (bob_dcgka_0, bob_0_seq_0) = {
        let seq = 0;
        let (bob_dcgka_0, bob_0) = Dcgka::process_remote(
            bob_dcgka,
            ProcessInput {
                seq,
                sender: alice,
                message: (&alice_0_seq_0, Some(bob)).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_create(&bob_dcgka_0, &bob_0, bob, alice, &[alice, bob], seq);
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
        let seq = 0;
        let (alice_dcgka_1, alice_1) = Dcgka::process_remote(
            alice_dcgka_0,
            ProcessInput {
                seq,
                sender: bob,
                message: (&bob_0_seq_0, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_ack(&alice_dcgka_1, &alice_1, alice, bob);
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

    let (bob_dcgka_1, bob_1_seq_1) = {
        let (bob_dcgka_pre, bob_pre) = Dcgka::add(bob_dcgka_0, charlie, &rng).unwrap();
        let seq = 1;
        let (bob_dcgka_1, bob_1) = Dcgka::process_local(bob_dcgka_pre, seq, bob_pre, &rng).unwrap();
        test.assert_add(&bob_dcgka_1, &bob_1, bob, charlie, &[alice, bob, charlie]);
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

    let (charlie_dcgka_0, charlie_0_seq_0) = {
        let seq = 1;
        let (charlie_dcgka_0, charlie_0) = Dcgka::process_remote(
            charlie_dcgka,
            ProcessInput {
                seq,
                sender: bob,
                message: (&bob_1_seq_1, Some(charlie)).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_welcome(
            &charlie_dcgka_0,
            &charlie_0,
            bob,
            charlie,
            &[alice, bob, charlie],
            seq,
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

    let (alice_dcgka_2, alice_2_seq_1) = {
        let seq = 1;
        let (alice_dcgka_2, alice_2) = Dcgka::process_remote(
            alice_dcgka_1,
            ProcessInput {
                seq,
                sender: bob,
                message: (&bob_1_seq_1, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_add(
            &alice_dcgka_2,
            &alice_2,
            alice,
            bob,
            charlie,
            &[alice, bob, charlie],
            seq,
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
        let seq = 0;
        let (bob_dcgka_2, bob_2) = Dcgka::process_remote(
            bob_dcgka_1,
            ProcessInput {
                seq,
                sender: charlie,
                message: (&charlie_0_seq_0, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_ack(&bob_dcgka_2, &bob_2, bob, charlie);
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
        let seq = 0;
        let (alice_dcgka_3, alice_3) = Dcgka::process_remote(
            alice_dcgka_2,
            ProcessInput {
                seq,
                sender: charlie,
                message: (&charlie_0_seq_0, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_ack(&alice_dcgka_3, &alice_3, alice, charlie);
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
        let seq = 1;
        let (charlie_dcgka_1, charlie_1) = Dcgka::process_remote(
            charlie_dcgka_0,
            ProcessInput {
                seq,
                sender: alice,
                message: (&alice_2_seq_1, Some(charlie)).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_add_ack(&charlie_dcgka_1, &charlie_1, charlie, alice);
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
        let seq = 1;
        let (bob_dcgka_3, bob_3) = Dcgka::process_remote(
            bob_dcgka_2,
            ProcessInput {
                seq,
                sender: alice,
                message: (&alice_2_seq_1, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_add_ack(&bob_dcgka_3, &bob_3, bob, alice);
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

    let (charlie_dcgka_2, charlie_2_seq_1) = {
        let (charlie_dcgka_pre, charlie_pre) = Dcgka::remove(charlie_dcgka_1, alice, &rng).unwrap();
        let seq = 1;
        let (charlie_dcgka_2, charlie_2) =
            Dcgka::process_local(charlie_dcgka_pre, seq, charlie_pre, &rng).unwrap();
        test.assert_remove(
            &charlie_dcgka_2,
            &charlie_2,
            charlie,
            alice,
            &[bob, charlie],
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

    let (bob_dcgka_4, bob_4_seq_2) = {
        let seq = 1;
        let (bob_dcgka_4, bob_4) = Dcgka::process_remote(
            bob_dcgka_3,
            ProcessInput {
                seq,
                sender: charlie,
                message: (&charlie_2_seq_1, Some(bob)).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_remove(&bob_dcgka_4, &bob_4, bob, charlie, &[bob, charlie], seq);
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
        let seq = 2;
        let (charlie_dcgka_3, charlie_3) = Dcgka::process_remote(
            charlie_dcgka_2,
            ProcessInput {
                seq,
                sender: bob,
                message: (&bob_4_seq_2, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_ack(&charlie_dcgka_3, &charlie_3, charlie, bob);
        (charlie_dcgka_3, charlie_3)
    };

    // ===================================
    // 14. Bob updates the group's secrets
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

    let (bob_dcgka_5, bob_5_seq_3) = {
        let (bob_dcgka_pre, bob_pre) = Dcgka::update(bob_dcgka_4, &rng).unwrap();
        let seq = 3;
        let (bob_dcgka_5, bob_5) = Dcgka::process_local(bob_dcgka_pre, seq, bob_pre, &rng).unwrap();
        test.assert_update(&bob_dcgka_5, &bob_5, bob, &[bob, charlie]);
        (bob_dcgka_5, bob_5)
    };

    // ==================================
    // 15. Charlie processes Bob's Update
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

    let (_charlie_dcgka_4, charlie_4_seq_2) = {
        let seq = 3;
        let (charlie_dcgka_4, charlie_4) = Dcgka::process_remote(
            charlie_dcgka_3,
            ProcessInput {
                seq,
                sender: bob,
                message: (&bob_5_seq_3, Some(charlie)).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_update(
            &charlie_dcgka_4,
            &charlie_4,
            charlie,
            bob,
            &[bob, charlie],
            3,
        );
        (charlie_dcgka_4, charlie_4)
    };

    // ===============================
    // 16. Bob processes Charlie's Ack
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
        let seq = 2;
        let (bob_dcgka_6, bob_6) = Dcgka::process_remote(
            bob_dcgka_5,
            ProcessInput {
                seq,
                sender: charlie,
                message: (&charlie_4_seq_2, None).try_into().unwrap(),
            },
            &rng,
        )
        .unwrap();
        test.assert_process_ack(&bob_dcgka_6, &bob_6, bob, charlie);
        (bob_dcgka_6, bob_6)
    };
}
