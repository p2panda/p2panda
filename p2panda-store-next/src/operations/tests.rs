// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::test_utils::TestLog;
use p2panda_core::{Hash, Operation, Topic};

use crate::operations::OperationStore;
use crate::sqlite::SqliteStoreBuilder;

#[tokio::test]
async fn insert_get_delete_operations() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let log = TestLog::new();

    let operation_1 = log.operation::<()>(b"hey", ());
    let operation_2 = log.operation::<()>(b"ho", ());
    let operation_3 = log.operation::<()>(b"let's", ());
    let operation_4 = log.operation::<()>(b"go!", ());

    // Insert
    // ~~~~~~

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log.id())
            .await
            .unwrap()
    );
    // Re-inserting the same operation returns false.
    assert!(
        !store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log.id())
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

    store.commit(permit).await.unwrap();

    // Has
    // ~~~

    assert!(
        OperationStore::<Operation<()>, Hash, Topic>::has_operation(&store, &operation_1.hash)
            .await
            .unwrap()
    );
    // Operation 2 was not inserted.
    assert!(
        !OperationStore::<Operation<()>, Hash, Topic>::has_operation(&store, &operation_2.hash)
            .await
            .unwrap()
    );

    // Get
    // ~~~

    assert_eq!(
        OperationStore::<Operation<()>, Hash, Topic>::get_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
        Some(operation_4.clone())
    );
    assert_eq!(
        OperationStore::<Operation<()>, Hash, Topic>::get_operation(&store, &operation_2.hash)
            .await
            .unwrap(),
        None
    );

    // Delete
    // ~~~~~~

    let permit = store.begin().await.unwrap();

    assert!(
        OperationStore::<Operation<()>, Hash, Topic>::delete_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
    );
    // Deleting the same item again returns false.
    assert!(
        !OperationStore::<Operation<()>, Hash, Topic>::delete_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
    );

    store.commit(permit).await.unwrap();
}
