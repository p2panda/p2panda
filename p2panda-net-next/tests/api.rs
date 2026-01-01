// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_net_next::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net_next::test_utils::{TestMemoryStore, TestTopicMap};
use p2panda_net_next::{AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery};

#[tokio::test]
async fn modular_api() {
    let private_key = PrivateKey::new();

    let address_book = AddressBook::builder().spawn().await.unwrap();

    let endpoint = Endpoint::builder(address_book.clone())
        .private_key(private_key)
        .network_id([42; 32])
        .spawn()
        .await
        .unwrap();

    // TODO: Should this rather be a "resolver" etc.? Discovery is a confusing term.
    let _mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
        .mode(MdnsDiscoveryMode::Active)
        .spawn()
        .await
        .unwrap();

    // TODO: Should this be ConfidentialDiscovery or TopicDiscovery etc.?
    let _discovery = Discovery::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await
        .unwrap();

    let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await
        .unwrap();

    let handle = gossip.stream([1; 32]).await.unwrap();
    handle.publish(b"Hello, Panda!").await.unwrap();
    let mut rx = handle.subscribe();

    tokio::spawn(async move {
        while let Ok(_bytes) = rx.recv().await {
            // ...
        }
    });

    let store = TestMemoryStore::new();
    let topic_map = TestTopicMap::new();

    let sync = LogSync::builder(store, topic_map, address_book, endpoint, gossip)
        .spawn()
        .await
        .unwrap();

    let handle = sync.stream([1; 32], true).await.unwrap();
    let mut rx = handle.subscribe().await.unwrap();

    tokio::spawn(async move {
        while let Ok(_event) = rx.recv().await {
            // ...
        }
    });
}
