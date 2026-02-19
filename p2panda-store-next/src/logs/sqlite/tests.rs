// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::test_utils::TestLog;
use p2panda_core::{Operation, PrivateKey};

use crate::logs::LogStore;
use crate::operations::OperationStore;
use crate::sqlite::{SqliteStore, SqliteStoreBuilder};

#[tokio::test]
async fn get_latest_entry() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log.id())
            .await
            .unwrap()
    );

    assert!(
        store
            .insert_operation(&operation_2.hash.clone(), operation_2.clone(), log.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let result = <SqliteStore<'_> as LogStore<Operation, _, _, _, _>>::get_latest_entry(
        &store,
        &log.author(),
        &log.id(),
    )
    .await
    .unwrap();

    assert_eq!(result, Some((operation_2.hash, operation_2.header.seq_num)));
}

#[tokio::test]
async fn get_log_heights() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let private_key = PrivateKey::new();

    // Create two separate logs which share the same author.
    let log_1 = TestLog::new_from_private_key(private_key.clone());
    let log_2 = TestLog::new_from_private_key(private_key.clone());

    let operation_1 = log_1.operation(b"first", ());
    let operation_2 = log_1.operation(b"second", ());
    let operation_3 = log_2.operation(b"third", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log_1.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_2.hash.clone(), operation_2.clone(), log_1.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash.clone(), operation_3.clone(), log_2.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let result = <SqliteStore<'_> as LogStore<Operation, _, _, _, _>>::get_log_heights(
        &store,
        &private_key.public_key(),
        &[log_1.id(), log_2.id()],
    )
    .await
    .unwrap();

    let mut expected_result = HashMap::new();
    expected_result.insert(log_1.id(), 1);
    expected_result.insert(log_2.id(), 0);

    assert_eq!(result, Some(expected_result));
}

#[tokio::test]
async fn get_log_size() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log.id())
            .await
            .unwrap()
    );

    assert!(
        store
            .insert_operation(&operation_2.hash.clone(), operation_2.clone(), log.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let (operations_num, size) =
        <SqliteStore<'_> as LogStore<Operation, _, _, _, _>>::get_log_size(
            &store,
            &log.author(),
            &log.id(),
            None,
            None,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(operations_num, 2);

    let expected_size = operation_1.header.to_bytes().len() as u64
        + operation_1.header.payload_size
        + operation_2.header.to_bytes().len() as u64
        + operation_2.header.payload_size;
    assert_eq!(size, expected_size);
}

#[tokio::test]
async fn get_log_entries() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());
    let operation_3 = log.operation(b"third", ());
    let operation_4 = log.operation(b"fourth", ());
    let operation_5 = log.operation(b"fifth", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_2.hash.clone(), operation_2.clone(), log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash.clone(), operation_3.clone(), log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_4.hash.clone(), operation_4.clone(), log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_5.hash.clone(), operation_5.clone(), log.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let log_entries = <SqliteStore<'_> as LogStore<Operation, _, _, _, _>>::get_log_entries(
        &store,
        &log.author(),
        &log.id(),
        None,
        None,
    )
    .await
    .expect("no errors");

    assert!(log_entries.is_some());
    let log_entries = log_entries.unwrap();

    assert_eq!(log_entries.len(), 5);

    assert_eq!(log_entries[0].0, operation_1);
    assert_eq!(log_entries[1].0, operation_2);
    assert_eq!(log_entries[2].0, operation_3);
    assert_eq!(log_entries[3].0, operation_4);
    assert_eq!(log_entries[4].0, operation_5);
}

#[tokio::test]
async fn prune_entries() {
    todo!()
}
