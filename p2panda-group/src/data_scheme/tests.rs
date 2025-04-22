// SPDX-License-Identifier: MIT OR Apache-2.0

use std::thread::sleep;
use std::time::Duration;

use crate::crypto::Rng;
use crate::crypto::x25519::SecretKey;
use crate::data_scheme::dcgka::{Dcgka, DcgkaError, DcgkaState, GroupSecretOutput, ProcessInput};
use crate::data_scheme::dgm::test_utils::TestDgm;
use crate::data_scheme::group_secret::GroupSecretBundle;
use crate::key_bundle::Lifetime;
use crate::key_manager::KeyManager;
use crate::key_registry::KeyRegistry;
use crate::message_scheme::test_utils::{MemberId, MessageId};
use crate::traits::PreKeyManager;

type TestDcgkaState = DcgkaState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    TestDgm<MemberId, MessageId>,
    KeyManager,
>;

#[test]
fn group_operations() {
    let rng = Rng::from_seed([1; 32]);

    let alice = 0;
    let bob = 1;
    let charlie = 2;

    // Alice initialises their key material.

    let alice_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

    let alice_dgm = TestDgm::init(alice);
    let alice_pki = KeyRegistry::init();
    let alice_keys = KeyManager::init(&alice_identity_secret, Lifetime::default(), &rng).unwrap();

    let alice_prekeys = KeyManager::prekey_bundle(&alice_keys);

    // Bob initialises their key material.

    let bob_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

    let bob_dgm = TestDgm::init(bob);
    let bob_pki = KeyRegistry::init();
    let bob_keys = KeyManager::init(&bob_identity_secret, Lifetime::default(), &rng).unwrap();

    let bob_prekeys = KeyManager::prekey_bundle(&bob_keys);

    // Charlie initialises their key material.

    let charlie_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

    let charlie_dgm = TestDgm::init(charlie);
    let charlie_pki = KeyRegistry::init();
    let charlie_keys =
        KeyManager::init(&charlie_identity_secret, Lifetime::default(), &rng).unwrap();

    let charlie_prekeys = KeyManager::prekey_bundle(&charlie_keys);

    // Register key bundles.

    let alice_pki = KeyRegistry::add_longterm_bundle(alice_pki, alice, alice_prekeys.clone());
    let alice_pki = KeyRegistry::add_longterm_bundle(alice_pki, bob, bob_prekeys.clone());
    let alice_pki = KeyRegistry::add_longterm_bundle(alice_pki, charlie, charlie_prekeys.clone());

    let bob_pki = KeyRegistry::add_longterm_bundle(bob_pki, alice, alice_prekeys.clone());
    let bob_pki = KeyRegistry::add_longterm_bundle(bob_pki, bob, bob_prekeys.clone());
    let bob_pki = KeyRegistry::add_longterm_bundle(bob_pki, charlie, charlie_prekeys.clone());

    let charlie_pki = KeyRegistry::add_longterm_bundle(charlie_pki, alice, alice_prekeys.clone());
    let charlie_pki = KeyRegistry::add_longterm_bundle(charlie_pki, bob, bob_prekeys.clone());
    let charlie_pki =
        KeyRegistry::add_longterm_bundle(charlie_pki, charlie, charlie_prekeys.clone());

    // Initialise DCGKA states.

    let mut alice_bundle = GroupSecretBundle::new();
    let alice_dcgka: TestDcgkaState = Dcgka::init(alice, alice_keys, alice_pki, alice_dgm);

    let mut bob_bundle = GroupSecretBundle::new();
    let bob_dcgka: TestDcgkaState = Dcgka::init(bob, bob_keys, bob_pki, bob_dgm);

    let mut charlie_bundle = GroupSecretBundle::new();
    let charlie_dcgka: TestDcgkaState =
        Dcgka::init(charlie, charlie_keys, charlie_pki, charlie_dgm);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Alice creates a group with Bob
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    let (alice_dcgka, output) = Dcgka::create(alice_dcgka, vec![alice, bob], &rng).unwrap();
    let alice_group_secret_0 = output.group_secret.unwrap();
    alice_bundle.insert(alice_group_secret_0.clone());
    assert_eq!(alice_bundle.len(), 1);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Bob processes Alice's "create" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    assert_eq!(output.control_message.to_string(), "create");

    let direct_message = output
        .direct_messages
        .into_iter()
        .find(|dm| dm.recipient == bob)
        .expect("direct message for bob");

    let (bob_dcgka, output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 0,
            },
            sender: alice,
            control_message: output.control_message.clone(),
            direct_message: Some(direct_message),
        },
    )
    .unwrap();

    let GroupSecretOutput::Secret(bob_group_secret_0) = output.group_secret else {
        panic!("expected group secret");
    };

    bob_bundle.insert(bob_group_secret_0.clone());
    assert_eq!(bob_bundle.len(), 1);

    // Alice and bob share the same group secret.
    assert_eq!(alice_group_secret_0, bob_group_secret_0);
    assert_eq!(alice_bundle, bob_bundle);

    // ~~~~~~~~~~~~~~~~
    // Bob adds Charlie
    // ~~~~~~~~~~~~~~~~

    let (bob_dcgka, add_output) = Dcgka::add(bob_dcgka, charlie, &bob_bundle, &rng).unwrap();
    assert!(add_output.group_secret.is_none());

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Charlie processes Bob's "add" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    assert_eq!(add_output.control_message.to_string(), "add");

    let direct_message = add_output
        .direct_messages
        .into_iter()
        .find(|dm| dm.recipient == charlie)
        .expect("direct message for charlie");

    let (charlie_dcgka, output) = Dcgka::process_remote(
        charlie_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: bob,
                seq: 0,
            },
            sender: bob,
            control_message: add_output.control_message.clone(),
            direct_message: Some(direct_message),
        },
    )
    .unwrap();

    let GroupSecretOutput::Bundle(charlie_secret_bundle_0) = output.group_secret else {
        panic!("expected group secret bundle");
    };

    charlie_bundle.extend(charlie_secret_bundle_0);
    assert_eq!(charlie_bundle.len(), 1);

    // Alice, Bob and Charlie share the same secrets.
    assert_eq!(alice_bundle, bob_bundle);
    assert_eq!(alice_bundle, charlie_bundle);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Alice processes Bob's "add" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    let (alice_dcgka, output) = Dcgka::process_remote(
        alice_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: bob,
                seq: 0,
            },
            sender: bob,
            control_message: add_output.control_message,
            direct_message: None,
        },
    )
    .unwrap();
    assert_eq!(output.group_secret, GroupSecretOutput::None);

    // ~~~~~~~~~~~~~~~~~~~~~~~
    // Alice updates the group
    // ~~~~~~~~~~~~~~~~~~~~~~~

    // Sleep for a moment to cause timestamp of generated group secret to be later than previously
    // generated ones.
    sleep(Duration::from_secs(1));

    let (alice_dcgka, update_output) = Dcgka::update(alice_dcgka, &rng).unwrap();
    assert_eq!(update_output.direct_messages.len(), 2); // dm's for Bob and Charlie

    let alice_group_secret_1 = update_output.group_secret.unwrap();
    alice_bundle.insert(alice_group_secret_1.clone());
    assert_eq!(alice_bundle.len(), 2);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Bob processes Alice's "update" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    assert_eq!(update_output.control_message.to_string(), "update");

    let direct_message = update_output
        .direct_messages
        .iter()
        .find(|dm| dm.recipient == bob)
        .expect("direct message for bob");

    let (bob_dcgka, output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 1,
            },
            sender: alice,
            control_message: update_output.control_message.clone(),
            direct_message: Some(direct_message.clone()),
        },
    )
    .unwrap();

    let GroupSecretOutput::Secret(bob_group_secret_1) = output.group_secret else {
        panic!("expected group secret");
    };

    bob_bundle.insert(bob_group_secret_1);
    assert_eq!(bob_bundle.len(), 2);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Charlie processes Alice's "update" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    let direct_message = update_output
        .direct_messages
        .into_iter()
        .find(|dm| dm.recipient == charlie)
        .expect("direct message for charlie");

    let (charlie_dcgka, output) = Dcgka::process_remote(
        charlie_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: alice,
                seq: 1,
            },
            sender: alice,
            control_message: update_output.control_message.clone(),
            direct_message: Some(direct_message.clone()),
        },
    )
    .unwrap();

    let GroupSecretOutput::Secret(charlie_group_secret_1) = output.group_secret else {
        panic!("expected group secret");
    };

    charlie_bundle.insert(charlie_group_secret_1);
    assert_eq!(charlie_bundle.len(), 2);

    // Alice, Bob and Charlie share the same secrets.
    assert_eq!(alice_bundle, bob_bundle);
    assert_eq!(alice_bundle, charlie_bundle);
    assert_eq!(alice_bundle.latest(), bob_bundle.latest());
    assert_eq!(charlie_bundle.latest(), bob_bundle.latest());

    // ~~~~~~~~~~~~~~~~~~~~~
    // Charlie removes Alice
    // ~~~~~~~~~~~~~~~~~~~~~

    // Sleep for a moment to cause timestamp of generated group secret to be later than previously
    // generated ones.
    sleep(Duration::from_secs(1));

    let (_charlie_dcgka, remove_output) = Dcgka::remove(charlie_dcgka, alice, &rng).unwrap();
    assert_eq!(remove_output.direct_messages.len(), 1);

    charlie_bundle.insert(remove_output.group_secret.unwrap());
    assert_eq!(charlie_bundle.len(), 3);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Bob processes Charlie's "remove" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    assert_eq!(remove_output.control_message.to_string(), "remove");

    let direct_message = remove_output
        .direct_messages
        .into_iter()
        .find(|dm| dm.recipient == bob)
        .expect("direct message for bob");

    let (_bob_dcgka, output) = Dcgka::process_remote(
        bob_dcgka,
        ProcessInput {
            seq: MessageId {
                sender: charlie,
                seq: 0,
            },
            sender: charlie,
            control_message: remove_output.control_message.clone(),
            direct_message: Some(direct_message.clone()),
        },
    )
    .unwrap();

    let GroupSecretOutput::Secret(bob_group_secret_2) = output.group_secret else {
        panic!("expected group secret");
    };

    bob_bundle.insert(bob_group_secret_2);
    assert_eq!(bob_bundle.len(), 3);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Alice tries to process Charlie's "remove" message
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    assert!(matches!(
        Dcgka::process_remote(
            alice_dcgka,
            ProcessInput {
                seq: MessageId {
                    sender: charlie,
                    seq: 0,
                },
                sender: charlie,
                control_message: remove_output.control_message,
                direct_message: Some(direct_message),
            },
        ),
        Err(DcgkaError::NotOurDirectMessage(_, _))
    ));

    // Bob and Charlie share the same secrets.
    assert_eq!(charlie_bundle, bob_bundle);
    assert_eq!(charlie_bundle.latest(), bob_bundle.latest());

    // Alice does not share the latest secrets with Bob and Charlie.
    assert_ne!(alice_bundle.latest(), bob_bundle.latest());
    assert_ne!(alice_bundle.latest(), charlie_bundle.latest());
}
