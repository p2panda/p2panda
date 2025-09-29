// SPDX-License-Identifier: MIT OR Apache-2.0

/// Macro to run the same test logic against all store backend implementations.
///
/// This macro takes a closure that will be executed against each store type:
/// - In-memory store (`MemoryStore`)
/// - SQLite store (`SqliteStore`)
///
/// ## Example
///
/// ```rust
/// # use crate::p2panda_store_next::orderer::OrdererStore;
/// # use crate::p2panda_store_next::orderer::OrdererTestExt;
/// # use p2panda_store_next::assert_all_stores;
/// # async fn run() {
/// assert_all_stores!(|store| async {
///     store.mark_ready("test".to_string()).await.unwrap();
///     assert_eq!(store.ready_len().await, 1);
/// });
/// # }
/// ```
#[macro_export]
macro_rules! assert_all_stores {
    (|$store:ident| $test_body:expr) => {
        // Test with MemoryStore.
        {
            let $store = $crate::memory::MemoryStore::<(), String>::default();
            $test_body.await;
        }

        // Test with SqliteStore.
        {
            let sqlite_store = $crate::sqlite::SqliteStoreBuilder::new()
                .random_memory_url()
                // We're running in a single test thread and can't have more parallel connections.
                .max_connections(1)
                .build()
                .await
                .unwrap();
            let permit = sqlite_store.begin().await.unwrap();
            let $store = sqlite_store.clone();
            $test_body.await;
            sqlite_store.commit(permit).await.unwrap();
        }
    };
}
