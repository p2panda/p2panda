// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::{PrivateKey, Topic};

use crate::SqliteStore;
use crate::topics::TopicStore;

#[tokio::test]
async fn update_and_resolve_topic_mapping() {
    let store = SqliteStore::temporary().await;

    let topic = Topic::new();

    // The log id is the same as the topic, in a use case like this there will be one log
    // per-author in each topic.
    let log_id = topic;

    let alice = PrivateKey::from_bytes(&[1u8; 32]).public_key();
    let bob = PrivateKey::from_bytes(&[2u8; 32]).public_key();

    let permit = store.begin().await.unwrap();

    let result = store.associate(&topic, &alice, &log_id).await.unwrap();
    assert!(result);

    let result = store.associate(&topic, &bob, &log_id).await.unwrap();
    assert!(result);

    store.commit(permit).await.unwrap();

    // Inserting bob again results in a false result.
    let permit = store.begin().await.unwrap();

    let result = store.associate(&topic, &bob, &log_id).await.unwrap();
    assert!(!result);

    store.commit(permit).await.unwrap();

    let expected_logs = HashMap::from([(alice, vec![topic]), (bob, vec![topic])]);

    let logs = store.resolve(&topic).await.unwrap();
    assert_eq!(logs, expected_logs);
}

#[tokio::test]
async fn path_based_log_ids() {
    let store = SqliteStore::temporary().await;

    let topic = Topic::new();

    // Here we demonstrate use cases where there are multiple logs per-author in each topic.
    let log_id_kittens = "kittens".to_string();
    let log_id_kittens_sleepy = "kittens.sleepy".to_string();
    let log_id_puppies = "puppies".to_string();

    let alice = PrivateKey::from_bytes(&[1u8; 32]).public_key();
    let bob = PrivateKey::from_bytes(&[2u8; 32]).public_key();

    let permit = store.begin().await.unwrap();

    let result = store
        .associate(&topic, &alice, &log_id_kittens)
        .await
        .unwrap();
    assert!(result);

    let result = store
        .associate(&topic, &alice, &log_id_kittens_sleepy)
        .await
        .unwrap();
    assert!(result);

    let result = store
        .associate(&topic, &bob, &log_id_puppies)
        .await
        .unwrap();
    assert!(result);

    store.commit(permit).await.unwrap();

    let expected_logs = HashMap::from([
        (alice, vec![log_id_kittens, log_id_kittens_sleepy]),
        (bob, vec![log_id_puppies]),
    ]);

    let logs = store.resolve(&topic).await.unwrap();
    assert_eq!(logs, expected_logs);
}

#[tokio::test]
async fn remove_association() {
    let store = SqliteStore::temporary().await;

    let topic = Topic::new();

    // Here we demonstrate use cases where there are multiple logs per-author in each topic.
    let log_id_kittens = "kittens".to_string();
    let log_id_kittens_sleepy = "kittens.sleepy".to_string();

    let alice = PrivateKey::from_bytes(&[1u8; 32]).public_key();

    let permit = store.begin().await.unwrap();

    let result = store
        .associate(&topic, &alice, &log_id_kittens)
        .await
        .unwrap();
    assert!(result);

    let result = store
        .associate(&topic, &alice, &log_id_kittens_sleepy)
        .await
        .unwrap();
    assert!(result);

    store.commit(permit).await.unwrap();

    let expected_logs = HashMap::from([(
        alice,
        vec![log_id_kittens.clone(), log_id_kittens_sleepy.clone()],
    )]);

    let logs = store.resolve(&topic).await.unwrap();
    assert_eq!(logs, expected_logs);

    let permit = store.begin().await.unwrap();

    let result = store
        .remove(&topic, &alice, &log_id_kittens_sleepy)
        .await
        .unwrap();

    store.commit(permit).await.unwrap();

    assert!(result);

    let expected_logs = HashMap::from([(alice, vec![log_id_kittens])]);

    let logs = store.resolve(&topic).await.unwrap();
    assert_eq!(logs, expected_logs);
}
