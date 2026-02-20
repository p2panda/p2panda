// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use tokio::time::sleep;

use crate::address_book::AddressBook;
use crate::iroh_endpoint::Endpoint;
use crate::iroh_mdns::{MdnsDiscovery, MdnsDiscoveryMode};
use crate::test_utils::{setup_logging, test_args_from_seed};

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
        .private_key(alice_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bob_endpoint = Endpoint::builder(bob_address_book.clone())
        .config(bob_args.iroh_config.clone())
        .private_key(bob_args.private_key.clone())
        .spawn()
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
    sleep(Duration::from_millis(1000)).await;

    // Alice should be in Bob's address book and vice-versa.
    let result = bob_address_book
        .node_info(alice_args.public_key)
        .await
        .unwrap();
    assert!(result.is_some());

    let result = alice_address_book
        .node_info(bob_args.public_key)
        .await
        .unwrap();
    assert!(result.is_some());
}
