// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use futures_util::StreamExt;
use p2panda_core::SigningKey;
use p2panda_core::test_utils::TestLog;

use crate::logs::{LogStore, StreamItem};
use crate::operations::OperationStore;
use crate::sqlite::SqliteStore;
use crate::traits::Transaction;

#[tokio::test]
async fn get_latest_entry() {
    let store = SqliteStore::temporary().await;

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash, &operation_1, &log.id())
            .await
            .unwrap()
    );

    assert!(
        store
            .insert_operation(&operation_2.hash, &operation_2, &log.id())
            .await
            .unwrap()
    );

    let result = store
        .get_latest_entry_tx(&log.author(), &log.id())
        .await
        .unwrap();

    store.commit(permit).await.unwrap();

    assert_eq!(result, Some(operation_2.into()));
}

#[tokio::test]
async fn get_log_heights() {
    let store = SqliteStore::temporary().await;

    let signing_key = SigningKey::generate();

    // Create two separate logs which share the same author.
    let log_1 = TestLog::from_signing_key(signing_key.clone());
    let log_2 = TestLog::from_signing_key(signing_key.clone());

    let operation_1 = log_1.operation(b"first", ());
    let operation_2 = log_1.operation(b"second", ());
    let operation_3 = log_2.operation(b"third", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash, &operation_1, &log_1.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_2.hash, &operation_2, &log_1.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash, &operation_3, &log_2.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let result = store
        .get_log_heights(&signing_key.verifying_key(), &[log_1.id(), log_2.id()])
        .await
        .unwrap();

    let expected_result = BTreeMap::from([(log_1.id(), 1), (log_2.id(), 0)]);

    assert_eq!(result, Some(expected_result));
}

#[tokio::test]
async fn get_log_size() {
    let store = SqliteStore::temporary().await;

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash, &operation_1, &log.id())
            .await
            .unwrap()
    );

    assert!(
        store
            .insert_operation(&operation_2.hash, &operation_2, &log.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let (operations_num, size) = store
        .get_log_size(&log.author(), &log.id(), None, None)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(operations_num, 2);

    let expected_size = operation_1.header.size() as u32
        + operation_1.header.payload_size
        + operation_2.header.size() as u32
        + operation_2.header.payload_size;
    assert_eq!(size, expected_size);
}

#[tokio::test]
async fn get_log_entries() {
    let store = SqliteStore::temporary().await;

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());
    let operation_3 = log.operation(b"third", ());
    let operation_4 = log.operation(b"fourth", ());
    let operation_5 = log.operation(b"fifth", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash, &operation_1, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_2.hash, &operation_2, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash, &operation_3, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_4.hash, &operation_4, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_5.hash, &operation_5, &log.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let mut log_entries = store
        .log_entries(&log.author(), &log.id(), None, None)
        .expect("no errors");

    let expected = [
        operation_1,
        operation_2,
        operation_3,
        operation_4,
        operation_5,
    ];
    for index in 0..=4 {
        let StreamItem { entry, .. } = log_entries.next().await.unwrap().unwrap();
        assert_eq!(entry, expected[index].clone().into());
    }

    assert!(log_entries.next().await.is_none());
}

#[tokio::test]
async fn prune_entries() {
    let store = SqliteStore::temporary().await;

    let log = TestLog::new();

    let operation_1 = log.operation(b"first", ());
    let operation_2 = log.operation(b"second", ());
    let operation_3 = log.operation(b"third", ());
    let operation_4 = log.operation(b"fourth", ());
    let operation_5 = log.operation(b"fifth", ());

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash, &operation_1, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_2.hash, &operation_2, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash, &operation_3, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_4.hash, &operation_4, &log.id())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_5.hash, &operation_5, &log.id())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    let prune_entries_num = SqliteStore::prune_entries(&store, &log.author(), &log.id(), &3)
        .await
        .expect("no errors");

    assert_eq!(prune_entries_num, 3);

    let mut log_entries = store
        .log_entries(&log.author(), &log.id(), None, None)
        .expect("no errors");

    // Three entries were pruned; the two most recently published entries should
    // remain.
    let StreamItem { entry, .. } = log_entries.next().await.unwrap().unwrap();
    assert_eq!(entry, operation_4.into());

    let StreamItem { entry, .. } = log_entries.next().await.unwrap().unwrap();
    assert_eq!(entry, operation_5.into());

    assert!(log_entries.next().await.is_none());
}
