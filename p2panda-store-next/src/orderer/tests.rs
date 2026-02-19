// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Hash;

use crate::orderer::OrdererStore;
use crate::{SqliteStore, Transaction};

#[tokio::test]
async fn ready() {
    let store = SqliteStore::temporary().await;

    let hash_1 = Hash::new(b"tick");
    let hash_2 = Hash::new(b"trick");
    let hash_3 = Hash::new(b"track");

    let permit = store.begin().await.unwrap();

    // 1. Mark three items as "ready".
    assert!(store.mark_ready(hash_3).await.unwrap());
    assert!(store.mark_ready(hash_2).await.unwrap());

    // Should return false when trying to insert the same item again.
    assert!(!store.mark_ready(hash_2).await.unwrap());

    // 2. Should correctly tell us if dependencies have been met.
    assert!(store.ready(&[hash_2, hash_3]).await.unwrap());
    assert!(!store.ready(&[hash_1, hash_3]).await.unwrap());
    assert!(!store.ready(&[hash_1]).await.unwrap());

    // 3. Check if they come out in the queued-up order (FIFO) when calling "take_next_ready".
    assert_eq!(store.take_next_ready().await.unwrap(), Some(hash_3));

    // .. another item got inserted "mid-way".
    assert!(store.mark_ready(hash_1).await.unwrap());

    assert_eq!(store.take_next_ready().await.unwrap(), Some(hash_2));
    assert_eq!(store.take_next_ready().await.unwrap(), Some(hash_1));
    assert_eq!(
        OrdererStore::<Hash>::take_next_ready(&store).await.unwrap(),
        None
    );

    store.commit(permit).await.unwrap();
}

#[tokio::test]
async fn pending() {
    let store = SqliteStore::temporary().await;

    let hash_1 = Hash::new(b"piff");
    let hash_2 = Hash::new(b"puff");
    let hash_3 = Hash::new(b"paff");
    let hash_4 = Hash::new(b"peff");

    let permit = store.begin().await.unwrap();

    // 1. Should correctly return true or false when insertion occured.
    assert!(
        store
            .mark_pending(hash_1, vec![hash_2, hash_3])
            .await
            .unwrap()
    );
    assert!(store.mark_pending(hash_1, vec![hash_3]).await.unwrap());
    assert!(!store.mark_pending(hash_1, vec![hash_3]).await.unwrap());
    assert!(
        store
            .mark_pending(hash_1, vec![hash_4, hash_3])
            .await
            .unwrap()
    );

    // 2. Return correct list of pending items.
    let pending = store.get_next_pending(hash_2).await.unwrap().unwrap();
    assert_eq!(pending.len(), 1);
    let (parent, deps) = pending.iter().next().unwrap();
    assert_eq!(*parent, hash_1);
    assert!(deps.contains(&hash_2));
    assert!(deps.contains(&hash_3));

    store.commit(permit).await.unwrap();
}
