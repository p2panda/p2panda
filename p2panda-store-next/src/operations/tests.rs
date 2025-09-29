// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::test_utils::TestLog;
use p2panda_core::{Hash, Operation};

use crate::memory::MemoryStore;
use crate::operations::OperationStore;
use crate::sqlite::SqliteStoreBuilder;

#[tokio::test]
async fn insert_get_delete_operations_memory() {
    let store = MemoryStore::new();

    let log = TestLog::new();

    let operation_1 = log.operation::<()>(b"hey", None);
    let operation_2 = log.operation::<()>(b"ho", None);
    let operation_3 = log.operation::<()>(b"let's", None);
    let operation_4 = log.operation::<()>(b"go!", None);

    // Insert
    // ~~~~~~

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone())
            .await
            .unwrap()
    );
    // Re-inserting the same operation returns false.
    assert!(
        !store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash.clone(), operation_3.clone())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_4.hash.clone(), operation_4.clone())
            .await
            .unwrap()
    );

    // Has
    // ~~~

    assert!(
        OperationStore::<Operation<()>, Hash>::has_operation(&store, &operation_1.hash)
            .await
            .unwrap()
    );
    // Operation 2 was not inserted.
    assert!(
        !OperationStore::<Operation<()>, Hash>::has_operation(&store, &operation_2.hash)
            .await
            .unwrap()
    );

    // Get
    // ~~~

    assert_eq!(
        store.get_operation(&operation_4.hash).await.unwrap(),
        Some(operation_4.clone())
    );
    assert_eq!(
        OperationStore::<Operation<()>, Hash>::get_operation(&store, &operation_2.hash)
            .await
            .unwrap(),
        None
    );

    // Delete
    // ~~~~~~

    assert!(
        OperationStore::<Operation<()>, Hash>::delete_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
    );
    // Deleting the same item again returns false.
    assert!(
        !OperationStore::<Operation<()>, Hash>::delete_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn insert_get_delete_operations_sqlite() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    let log = TestLog::new();

    let operation_1 = log.operation::<()>(b"hey", None);
    let operation_2 = log.operation::<()>(b"ho", None);
    let operation_3 = log.operation::<()>(b"let's", None);
    let operation_4 = log.operation::<()>(b"go!", None);

    // Insert
    // ~~~~~~

    let permit = store.begin().await.unwrap();

    assert!(
        store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone())
            .await
            .unwrap()
    );
    // Re-inserting the same operation returns false.
    assert!(
        !store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_3.hash.clone(), operation_3.clone())
            .await
            .unwrap()
    );
    assert!(
        store
            .insert_operation(&operation_4.hash.clone(), operation_4.clone())
            .await
            .unwrap()
    );

    store.commit(permit).await.unwrap();

    // Has
    // ~~~

    assert!(
        OperationStore::<Operation<()>, Hash>::has_operation(&store, &operation_1.hash)
            .await
            .unwrap()
    );
    // Operation 2 was not inserted.
    assert!(
        !OperationStore::<Operation<()>, Hash>::has_operation(&store, &operation_2.hash)
            .await
            .unwrap()
    );

    // Get
    // ~~~

    assert_eq!(
        store.get_operation(&operation_4.hash).await.unwrap(),
        Some(operation_4.clone())
    );
    assert_eq!(
        OperationStore::<Operation<()>, Hash>::get_operation(&store, &operation_2.hash)
            .await
            .unwrap(),
        None
    );

    // Delete
    // ~~~~~~

    let permit = store.begin().await.unwrap();

    assert!(
        OperationStore::<Operation<()>, Hash>::delete_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
    );
    // Deleting the same item again returns false.
    assert!(
        !OperationStore::<Operation<()>, Hash>::delete_operation(&store, &operation_4.hash)
            .await
            .unwrap(),
    );

    store.commit(permit).await.unwrap();
}
