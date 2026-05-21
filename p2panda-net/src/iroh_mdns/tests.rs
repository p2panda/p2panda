// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::test_utils::setup_logging;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::{MdnsDiscovery, MdnsDiscoveryMode};
use crate::test_utils::test_args_from_seed;

#[tokio::test]
async fn mdns_discovery() {
    setup_logging();

    let alice_args = test_args_from_seed([100; 32]);
    let bob_args = test_args_from_seed([200; 32]);

    // Spawn address book (it's a dependency) for both.
    let alice_address_book = AddressBook::builder().spawn().await.unwrap();
    let bob_address_book = AddressBook::builder().spawn().await.unwrap();

    // Spawn both endpoint actors.
    let alice_endpoint = Endpoint::builder(alice_address_book.clone())
        .config(alice_args.iroh_config.clone())
        .signing_key(alice_args.signing_key.clone())
        .spawn()
        .await
        .unwrap();
    let bob_endpoint = Endpoint::builder(bob_address_book.clone())
        .config(bob_args.iroh_config.clone())
        .signing_key(bob_args.signing_key.clone())
        .spawn()
        .await
        .unwrap();

    // Alice and Bob do not yet know about one another.
    let result = bob_address_book
        .node_info(alice_args.verifying_key)
        .await
        .unwrap();
    assert!(result.is_none());

    let result = alice_address_book
        .node_info(bob_args.verifying_key)
        .await
        .unwrap();
    assert!(result.is_none());

    // Listen for changes to Bob's node info in Alice's address book.
    let mut alice_address_book_bob = alice_address_book
        .watch_node_info(bob_endpoint.node_id(), true)
        .await
        .unwrap();

    // Listen for changes to Alice's node info in Bob's address book.
    let mut bob_address_book_alice = bob_address_book
        .watch_node_info(alice_endpoint.node_id(), true)
        .await
        .unwrap();

    // Enable active discovery mode, otherwise they'll not find each other.
    let _alice_mdns = MdnsDiscovery::builder(alice_address_book.clone(), alice_endpoint.clone())
        .mode(MdnsDiscoveryMode::Active)
        .spawn()
        .await
        .unwrap();
    let _bob_mdns = MdnsDiscovery::builder(bob_address_book.clone(), bob_endpoint.clone())
        .mode(MdnsDiscoveryMode::Active)
        .spawn()
        .await
        .unwrap();

    // Wait until they find each other and exchange transport infos.
    alice_address_book_bob.recv().await;
    bob_address_book_alice.recv().await;

    // Alice should be in Bob's address book and vice-versa.
    let result = bob_address_book
        .node_info(alice_args.verifying_key)
        .await
        .unwrap();
    assert!(result.is_some());

    let result = alice_address_book
        .node_info(bob_args.verifying_key)
        .await
        .unwrap();
    assert!(result.is_some());
}
