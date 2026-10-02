// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use p2panda_core::SigningKey;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::address_book::test_utils::{TestNodeId, TestNodeInfo, current_timestamp};
use crate::address_book::{AddressBookStore, NodeInfo};
use crate::{SqliteStore, Transaction};

#[tokio::test]
async fn insert_node_info() {
    let store = SqliteStore::temporary().await;

    let permit = store.begin().await.unwrap();

    let node_id = SigningKey::generate().verifying_key();
    let node_info = TestNodeInfo::new(node_id);

    let result = store.insert_node_info(node_info.clone()).await.unwrap();
    assert!(result);

    store.commit(permit).await.unwrap();

    assert_eq!(
        store.node_info(&node_info.id).await.unwrap(),
        Some(node_info.clone())
    );
}

#[tokio::test]
async fn ignore_stale_entries() {
    let store = SqliteStore::temporary().await;

    let permit = store.begin().await.unwrap();

    let node_id = SigningKey::generate().verifying_key();
    let node_info = TestNodeInfo::new(node_id).stale();

    let result = store.insert_node_info(node_info.clone()).await.unwrap();
    assert!(result);

    store.commit(permit).await.unwrap();

    assert_eq!(
        <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::all_nodes_len(&store,)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn set_and_query_topics() {
    let store = SqliteStore::temporary().await;

    let billie = SigningKey::generate().verifying_key();
    let daphne = SigningKey::generate().verifying_key();
    let carlos = SigningKey::generate().verifying_key();

    let cats = [100; 32].into();
    let dogs = [102; 32].into();
    let rain = [104; 32].into();
    let frogs = [106; 32].into();
    let trains = [200; 32].into();

    let permit = store.begin().await.unwrap();

    store
        .insert_node_info(TestNodeInfo::new(billie))
        .await
        .unwrap();

    <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
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

    <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
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

    <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
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
            .collect::<HashSet<TestNodeId>>(),
        HashSet::from_iter([billie, carlos])
    );

    assert_eq!(
        store
            .node_infos_by_topics(&[frogs, rain])
            .await
            .unwrap()
            .into_iter()
            .map(|item: TestNodeInfo| item.id)
            .collect::<HashSet<TestNodeId>>(),
        HashSet::from_iter([billie, carlos, daphne])
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
    let store = SqliteStore::temporary().await;

    let billie = SigningKey::generate().verifying_key();
    let daphne = SigningKey::generate().verifying_key();

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
    let result = <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::remove_older_than(
        &store,
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    assert_eq!(result, 1);

    store.commit(permit).await.unwrap();

    assert!(
        <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::node_info(&store, &billie)
            .await
            .unwrap()
            .is_none(),
    );
    assert!(
        <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::node_info(&store, &daphne)
            .await
            .unwrap()
            .is_some(),
    );
}

#[tokio::test]
async fn sample_random_nodes() {
    let store = SqliteStore::temporary().await;

    let mut rng = ChaCha20Rng::from_seed([1; 32]);

    let permit = store.begin().await.unwrap();

    for _ in 0..100 {
        let id = SigningKey::generate().verifying_key();
        store
            .insert_node_info(TestNodeInfo::new(id).with_random_address(&mut rng))
            .await
            .unwrap();
    }

    for _ in 200..300 {
        let id = SigningKey::generate().verifying_key();
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
            <SqliteStore as AddressBookStore<TestNodeId, TestNodeInfo>>::random_node(&store)
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

/// Encodes a node info record as a CBOR map with keys in struct-declaration order, which is *not*
/// canonical CBOR key order. This mimics records written by earlier `ciborium`-based versions.
fn encode_non_canonical(info: &TestNodeInfo) -> Vec<u8> {
    let mut buf = vec![0xa4]; // definite-length map with four entries
    for (key, value) in [
        ("id", cbor_core::Value::serialized(&info.id).unwrap()),
        (
            "bootstrap",
            cbor_core::Value::serialized(&info.bootstrap).unwrap(),
        ),
        ("stale", cbor_core::Value::serialized(&info.stale).unwrap()),
        (
            "transports",
            cbor_core::Value::serialized(&info.transports).unwrap(),
        ),
    ] {
        buf.extend(cbor_core::Value::from(key).encode());
        buf.extend(value.encode());
    }
    buf
}

#[tokio::test]
async fn decode_legacy_non_canonical_node_info() {
    let mut rng = ChaCha20Rng::from_seed([1; 32]);

    let node_id = SigningKey::generate().verifying_key();
    let node_info = TestNodeInfo::new(node_id).with_random_address(&mut rng);
    let bytes = encode_non_canonical(&node_info);

    // Insert a record with non-canonical map key ordering directly, then read it back through the
    // store to confirm the sqlite node info decoding tolerates it.
    let store = SqliteStore::temporary().await;
    sqlx::query(
        "
        INSERT INTO
            node_infos_v1 (node_id, node_info, bootstrap, stale)
        VALUES
            (?, ?, ?, ?)
        ",
    )
    .bind(node_id.to_hex())
    .bind(bytes)
    .bind(node_info.is_bootstrap())
    .bind(node_info.is_stale())
    .execute(&store.pool)
    .await
    .unwrap();

    assert_eq!(store.node_info(&node_id).await.unwrap(), Some(node_info));
}
