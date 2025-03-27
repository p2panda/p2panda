#![no_main]

use libfuzzer_sys::fuzz_target;

use p2panda_group::test_utils::SecretKey;
use p2panda_group::traits::PreKeyManager;
use p2panda_group::{KeyManager, Lifetime, OneTimeTwoParty, Rng};

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

    for action in actions {
        // Generate a random message with 128 characters.
        let expected: Vec<u8> = rng.random_vec(128).unwrap();

        // Randomly decide if Alice sends a message to Bob or vice versa.
        if action & 1 == 0 {
            let (alice_2sm_i, message) =
                OneTimeTwoParty::send(alice_2sm, &alice_manager, &expected, &rng).unwrap();
            let (bob_2sm_i, bob_manager_i, received) =
                OneTimeTwoParty::receive(bob_2sm, bob_manager, message).unwrap();

            alice_2sm = alice_2sm_i;
            bob_2sm = bob_2sm_i;
            bob_manager = bob_manager_i;

            assert_eq!(expected, received);
        } else {
            let (bob_2sm_i, message) =
                OneTimeTwoParty::send(bob_2sm, &bob_manager, &expected, &rng).unwrap();
            let (alice_2sm_i, alice_manager_i, received) =
                OneTimeTwoParty::receive(alice_2sm, alice_manager, message).unwrap();

            bob_2sm = bob_2sm_i;
            alice_2sm = alice_2sm_i;
            alice_manager = alice_manager_i;

            assert_eq!(expected, received);
        };
    }
});
