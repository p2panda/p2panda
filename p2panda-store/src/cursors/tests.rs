// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::logs::LogHeights;
use p2panda_core::{Cursor, SigningKey, VerifyingKey};

use crate::cursors::CursorStore;
use crate::{SqliteStore, tx_unwrap};

#[tokio::test]
async fn get_and_set_cursor() {
    let store = SqliteStore::temporary().await;

    let mut cursor = Cursor::<VerifyingKey, u64>::new("test", LogHeights::default());

    // First insert.
    tx_unwrap!(store, {
        store.set_cursor(&cursor).await.unwrap();
    });

    assert_eq!(
        store.get_cursor("test").await.unwrap(),
        Some(cursor.clone())
    );

    // Second insert should be an upsert.
    let author = SigningKey::generate().verifying_key();
    let log_id = 2;
    let log_height = 22;

    cursor.advance(author, log_id, log_height);

    tx_unwrap!(store, {
        store.set_cursor(&cursor).await.unwrap();
    });

    let cursor_2: Cursor<VerifyingKey, u64> = store
        .get_cursor("test")
        .await
        .unwrap()
        .expect("cursor should exist");

    assert_eq!(cursor_2.log_height(&author, &log_id), Some(&log_height));

    // Remove cursor.
    tx_unwrap!(store, {
        <SqliteStore as CursorStore<VerifyingKey, u64>>::delete_cursor(&store, "test")
            .await
            .unwrap();
    });

    assert!(
        <SqliteStore as CursorStore<VerifyingKey, u64>>::get_cursor(&store, "test")
            .await
            .unwrap()
            .is_none()
    );
}
