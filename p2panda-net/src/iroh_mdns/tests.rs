// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use p2panda_core::test_utils::setup_logging;

use crate::NetworkId;
use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::{MdnsDiscovery, MdnsDiscoveryMode};
use crate::test_utils::{ApplicationArguments, test_args_from_seed};

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

#[tokio::test]
async fn mdns_network_separation() {
    setup_logging();

    let alice_args = test_args_from_seed([101; 32]);
    let bob_args = test_args_from_seed([102; 32]);
    let charlie_args = test_args_from_seed([103; 32]);

    // Alice and Bob share one network, Charlie is part of a different one.
    let network_a: NetworkId = [11; 32];
    let network_b: NetworkId = [22; 32];

    let spawn = |args: ApplicationArguments, network_id: NetworkId| async move {
        let address_book = AddressBook::builder().spawn().await.unwrap();

        let endpoint = Endpoint::builder(address_book.clone())
            .config(args.iroh_config.clone())
            .signing_key(args.signing_key.clone())
            .network_id(network_id)
            .spawn()
            .await
            .unwrap();

        let mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
            .mode(MdnsDiscoveryMode::Active)
            .spawn()
            .await
            .unwrap();

        (address_book, endpoint, mdns)
    };

    let (alice_address_book, _alice_endpoint, _alice_mdns) =
        spawn(alice_args.clone(), network_a).await;
    let (bob_address_book, _bob_endpoint, _bob_mdns) = spawn(bob_args.clone(), network_a).await;
    let (charlie_address_book, _charlie_endpoint, _charlie_mdns) =
        spawn(charlie_args.clone(), network_b).await;

    // Wait until Alice and Bob found each other via mDNS.
    let mut alice_address_book_bob = alice_address_book
        .watch_node_info(bob_args.verifying_key, false)
        .await
        .unwrap();
    let mut bob_address_book_alice = bob_address_book
        .watch_node_info(alice_args.verifying_key, false)
        .await
        .unwrap();

    let timeout = Duration::from_secs(10);
    loop {
        let event = tokio::time::timeout(timeout, alice_address_book_bob.recv())
            .await
            .expect("alice should discover bob within the same network")
            .unwrap();

        if event.value.is_some() {
            break;
        }
    }

    loop {
        let event = tokio::time::timeout(timeout, bob_address_book_alice.recv())
            .await
            .expect("bob should discover alice within the same network")
            .unwrap();

        if event.value.is_some() {
            break;
        }
    }

    // Charlie is in a separate network and should not discover Alice and Bob.
    let mut charlie_address_book_any = charlie_address_book
        .watch_node_info(alice_args.verifying_key, true)
        .await
        .unwrap();
    let result =
        tokio::time::timeout(Duration::from_secs(3), charlie_address_book_any.recv()).await;
    assert!(
        result.is_err(),
        "Charlie doesn't discover nodes of a different network"
    );

    for node_id in [alice_args.verifying_key, bob_args.verifying_key] {
        let result = charlie_address_book.node_info(node_id).await.unwrap();

        assert!(
            result.is_none(),
            "Charlie does not discover nodes of network A"
        );
    }

    for address_book in [&alice_address_book, &bob_address_book] {
        let result = address_book
            .node_info(charlie_args.verifying_key)
            .await
            .unwrap();

        assert!(
            result.is_none(),
            "Nodes of network B do not discover Charlie"
        );
    }
}
