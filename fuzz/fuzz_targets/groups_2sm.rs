// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::collections::VecDeque;

use libfuzzer_sys::fuzz_target;
use p2panda_encryption::Rng;
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::test_utils::crypto::SecretKey;
use p2panda_encryption::traits::PreKeyManager;
use p2panda_encryption::two_party::{OneTimeTwoParty, TwoPartyMessage};

/// Max. number of messages in a member's inbox.
const INBOX_CAPACITY: usize = 128;

/// Assertable 2SM message.
struct Message {
    /// Expected message in plaintext.
    expected: Vec<u8>,

    /// Encrypted message.
    ciphertext: TwoPartyMessage,
}

// Create a 2SM session between Alice and Bob and randomly choose who sends a message to each
// other. The number of total actions per session is also picked randomly (1-128).
fuzz_target!(|args: ([u8; 32], &[u8])| {
    let (seed, actions) = args;

    let rng = Rng::from_seed(seed);

    // Alice generates their long-term key material.

    let alice_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
    let alice_manager =
        KeyManager::init(&alice_identity_secret, Lifetime::default(), &rng).unwrap();

    let (mut alice_manager, alice_prekey_bundle) =
        KeyManager::generate_onetime_bundle(alice_manager, &rng).unwrap();

    // Bob generates their long-term key material.

    let bob_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
    let bob_manager = KeyManager::init(&bob_identity_secret, Lifetime::default(), &rng).unwrap();

    let (mut bob_manager, bob_prekey_bundle) =
        KeyManager::generate_onetime_bundle(bob_manager, &rng).unwrap();

    // Alice and Bob set up the 2SM protocol handlers for each other.

    let mut alice_2sm = OneTimeTwoParty::init(bob_prekey_bundle.clone());
    let mut bob_2sm = OneTimeTwoParty::init(alice_prekey_bundle.clone());

    let mut to_alice_inbox = VecDeque::<Message>::with_capacity(INBOX_CAPACITY);
    let mut to_bob_inbox = VecDeque::<Message>::with_capacity(INBOX_CAPACITY);

    for action in actions {
        match action % 4 {
            0 => {
                // Only send to Bob if their inbox is not too full.
                if to_bob_inbox.len() >= INBOX_CAPACITY {
                    continue;
                }

                // Alice sends a random message with 32 characters to Bob.
                let expected: Vec<u8> = rng.random_vec(32).unwrap();

                let (alice_2sm_i, ciphertext) =
                    OneTimeTwoParty::send(alice_2sm, &alice_manager, &expected, &rng).unwrap();

                alice_2sm = alice_2sm_i;

                to_bob_inbox.push_back(Message {
                    expected,
                    ciphertext,
                });
            }
            1 => {
                // Only send to Alice if their inbox is not too full.
                if to_alice_inbox.len() >= INBOX_CAPACITY {
                    continue;
                }

                // Bob sends a random message with 32 characters to Alice.
                let expected: Vec<u8> = rng.random_vec(32).unwrap();

                let (bob_2sm_i, ciphertext) =
                    OneTimeTwoParty::send(bob_2sm, &bob_manager, &expected, &rng).unwrap();

                bob_2sm = bob_2sm_i;

                to_alice_inbox.push_back(Message {
                    expected,
                    ciphertext,
                });
            }
            2 => {
                // Alice reads one message in their inbox if there is anything.
                let Some(message) = to_alice_inbox.pop_front() else {
                    break;
                };

                let (alice_2sm_i, alice_manager_i, received) =
                    OneTimeTwoParty::receive(alice_2sm, alice_manager, message.ciphertext).unwrap();

                alice_2sm = alice_2sm_i;
                alice_manager = alice_manager_i;

                assert_eq!(message.expected, received);
            }
            3 => {
                // Bob reads one message in their inbox if there is anything.
                let Some(message) = to_bob_inbox.pop_front() else {
                    break;
                };

                let (bob_2sm_i, bob_manager_i, received) =
                    OneTimeTwoParty::receive(bob_2sm, bob_manager, message.ciphertext).unwrap();

                bob_2sm = bob_2sm_i;
                bob_manager = bob_manager_i;

                assert_eq!(message.expected, received);
            }
            _ => unreachable!(),
        }
    }
});
