// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::address_book::AddressBookMemoryStore;
use crate::address_book::test_utils::{TestNodeId, TestNodeInfo};

use super::memory::current_timestamp;
use super::{AddressBookStore, NodeInfo};

#[tokio::test]
async fn insert_node_info() {
    let rng = ChaCha20Rng::from_seed([1; 32]);
    let store = AddressBookMemoryStore::new(rng);

    let node_info_1 = TestNodeInfo::new(1);

    let result = store.insert_node_info(node_info_1.clone()).await.unwrap();

    assert!(result);
    assert_eq!(
        store.node_info(&node_info_1.id).await.unwrap(),
        Some(node_info_1.clone())
    );
}

#[tokio::test]
async fn set_and_query_topics() {
    let rng = ChaCha20Rng::from_seed([1; 32]);
    let store = AddressBookMemoryStore::new(rng);

    let cats = [100; 32];
    let dogs = [102; 32];
    let rain = [104; 32];
    let frogs = [106; 32];
    let trains = [200; 32];

    store.insert_node_info(TestNodeInfo::new(1)).await.unwrap();
    store
        .set_topics(1, HashSet::from_iter([cats, dogs, rain]))
        .await
        .unwrap();

    store.insert_node_info(TestNodeInfo::new(2)).await.unwrap();
    store
        .set_topics(2, HashSet::from_iter([rain]))
        .await
        .unwrap();

    store.insert_node_info(TestNodeInfo::new(3)).await.unwrap();
    store
        .set_topics(3, HashSet::from_iter([dogs, frogs]))
        .await
        .unwrap();

    assert_eq!(
        store
            .node_infos_by_topics(&[dogs])
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<TestNodeId>>(),
        vec![1, 3]
    );

    assert_eq!(
        store
            .node_infos_by_topics(&[frogs, rain])
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<TestNodeId>>(),
        vec![1, 2, 3]
    );

    assert!(
        store
            .node_infos_by_topics(&[trains])
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<TestNodeId>>()
            .is_empty()
    );
}

#[tokio::test]
async fn remove_outdated_node_infos() {
    let rng = ChaCha20Rng::from_seed([1; 32]);
    let store = AddressBookMemoryStore::new(rng);

    store.insert_node_info(TestNodeInfo::new(1)).await.unwrap();
    store
        .set_last_changed(1, current_timestamp() - (60 * 2))
        .await; // 2 minutes "old"

    // Timestamp of this entry will be set to "now" automatically.
    store.insert_node_info(TestNodeInfo::new(2)).await.unwrap();

    // Expect removing one item from database.
    let result = store
        .remove_older_than(Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(result, 1);
    assert!(store.node_info(&1).await.unwrap().is_none());
    assert!(store.node_info(&2).await.unwrap().is_some());
}

#[tokio::test]
async fn sample_random_nodes() {
    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    let store = AddressBookMemoryStore::new(rng.clone());

    for id in 0..100 {
        store
            .insert_node_info(TestNodeInfo::new(id).with_random_address(&mut rng))
            .await
            .unwrap();
    }

    for id in 200..300 {
        store
            .insert_node_info(TestNodeInfo::new_bootstrap(id).with_random_address(&mut rng))
            .await
            .unwrap();
    }

    // Sampling random nodes should give us some variety.
    for _ in 0..100 {
        assert_ne!(
            store.random_node().await.unwrap().unwrap(),
            store.random_node().await.unwrap().unwrap(),
        );
    }

    for _ in 0..100 {
        let sample_1 = store.random_bootstrap_node().await.unwrap().unwrap();
        let sample_2 = store.random_bootstrap_node().await.unwrap().unwrap();
        assert_ne!(sample_1, sample_2,);
        assert!(sample_1.is_bootstrap());
        assert!(sample_2.is_bootstrap());
    }
}
