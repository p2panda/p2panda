// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use p2panda_core::Hash;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::address_book::test_utils::{TestNodeId, TestNodeInfo, current_timestamp};
use crate::address_book::{AddressBookStore, NodeInfo};
use crate::sqlite::{SqliteStore, SqliteStoreBuilder};

#[tokio::test]
async fn insert_node_info() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let permit = store.begin().await.unwrap();

    let node_info_1 = TestNodeInfo::new(Hash::new(b"turtle"));
    let result = store.insert_node_info(node_info_1.clone()).await.unwrap();

    store.commit(permit).await.unwrap();

    assert!(result);
    assert_eq!(
        store.node_info(&node_info_1.id).await.unwrap(),
        Some(node_info_1.clone())
    );
}

#[tokio::test]
async fn set_and_query_topics() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let billie = Hash::new(b"billie");
    let daphne = Hash::new(b"daphne");
    let carlos = Hash::new(b"carlos");

    let cats = [100; 32];
    let dogs = [102; 32];
    let rain = [104; 32];
    let frogs = [106; 32];
    let trains = [200; 32];

    let permit = store.begin().await.unwrap();

    store
        .insert_node_info(TestNodeInfo::new(billie))
        .await
        .unwrap();

    <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
        &store,
        billie,
        HashSet::from_iter([cats, dogs, rain]),
    )
    .await
    .unwrap();

    store
        .insert_node_info(TestNodeInfo::new(daphne))
        .await
        .unwrap();

    <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
        &store,
        daphne,
        HashSet::from_iter([rain]),
    )
    .await
    .unwrap();

    store
        .insert_node_info(TestNodeInfo::new(carlos))
        .await
        .unwrap();

    <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
        &store,
        carlos,
        HashSet::from_iter([dogs, frogs]),
    )
    .await
    .unwrap();

    store.commit(permit).await.unwrap();

    assert_eq!(
        store
            .node_infos_by_topics(&[dogs])
            .await
            .unwrap()
            .into_iter()
            .map(|item: TestNodeInfo| item.id)
            .collect::<Vec<TestNodeId>>(),
        vec![billie, carlos]
    );

    assert_eq!(
        store
            .node_infos_by_topics(&[frogs, rain])
            .await
            .unwrap()
            .into_iter()
            .map(|item: TestNodeInfo| item.id)
            .collect::<Vec<TestNodeId>>(),
        vec![daphne, billie, carlos]
    );

    assert!(
        store
            .node_infos_by_topics(&[trains])
            .await
            .unwrap()
            .into_iter()
            .map(|item: TestNodeInfo| item.id)
            .collect::<Vec<TestNodeId>>()
            .is_empty()
    );
}

#[tokio::test]
async fn remove_outdated_node_infos() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let billie = Hash::new(b"billie");
    let daphne = Hash::new(b"daphne");

    let permit = store.begin().await.unwrap();

    store
        .insert_node_info(TestNodeInfo::new(billie))
        .await
        .unwrap();
    store
        .set_last_changed(&billie, current_timestamp() - (60 * 2))
        .await
        .unwrap(); // 2 minutes "old"

    // Timestamp of this entry will be set to "now" automatically.
    store
        .insert_node_info(TestNodeInfo::new(daphne))
        .await
        .unwrap();

    store.commit(permit).await.unwrap();

    let permit = store.begin().await.unwrap();

    // Expect removing one item from database.
    let result =
        <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::remove_older_than(
            &store,
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert_eq!(result, 1);

    store.commit(permit).await.unwrap();

    assert!(
        <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::node_info(&store, &billie)
            .await
            .unwrap()
            .is_none(),
    );
    assert!(
        <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::node_info(&store, &daphne)
            .await
            .unwrap()
            .is_some(),
    );
}

#[tokio::test]
async fn sample_random_nodes() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let mut rng = ChaCha20Rng::from_seed([1; 32]);

    let permit = store.begin().await.unwrap();

    for id in 0..100 {
        let id = Hash::new((id as usize).to_ne_bytes());
        store
            .insert_node_info(TestNodeInfo::new(id).with_random_address(&mut rng))
            .await
            .unwrap();
    }

    for id in 200..300 {
        let id = Hash::new((id as usize).to_ne_bytes());
        store
            .insert_node_info(TestNodeInfo::new_bootstrap(id).with_random_address(&mut rng))
            .await
            .unwrap();
    }

    store.commit(permit).await.unwrap();

    // Sampling random nodes should give us some variety.
    let mut samples = HashSet::new();
    for _ in 0..100 {
        samples.insert(
            <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::random_node(&store)
                .await
                .unwrap(),
        );
    }
    assert!(samples.len() > 25);

    let mut samples = HashSet::new();
    for _ in 0..100 {
        let sample: TestNodeInfo = store.random_bootstrap_node().await.unwrap().unwrap();
        assert!(sample.is_bootstrap());
        samples.insert(sample);
    }
    assert!(samples.len() > 25);
}
