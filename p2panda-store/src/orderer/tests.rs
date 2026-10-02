// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Hash;

use crate::orderer::OrdererStore;
use crate::{SqliteStore, Transaction};

#[tokio::test]
async fn ready() {
    let store = SqliteStore::temporary().await;

    let namespace = "default";

    let hash_1 = Hash::digest(b"tick");
    let hash_2 = Hash::digest(b"trick");
    let hash_3 = Hash::digest(b"track");

    let permit = store.begin().await.unwrap();

    // 1. Mark three items as "ready".
    assert!(store.mark_ready(namespace, hash_3).await.unwrap());
    assert!(store.mark_ready(namespace, hash_2).await.unwrap());

    // Should return false when trying to insert the same item again.
    assert!(!store.mark_ready(namespace, hash_2).await.unwrap());

    // 2. Should correctly tell us if dependencies have been met.
    assert!(store.ready(namespace, &[hash_2, hash_3]).await.unwrap());
    assert!(!store.ready(namespace, &[hash_1, hash_3]).await.unwrap());
    assert!(!store.ready(namespace, &[hash_1]).await.unwrap());

    // 3. Check if they come out in the queued-up order (FIFO) when calling "take_next_ready".
    assert_eq!(
        store.take_next_ready(namespace).await.unwrap(),
        Some(hash_3)
    );

    // .. another item got inserted "mid-way".
    assert!(store.mark_ready(namespace, hash_1).await.unwrap());

    assert_eq!(
        store.take_next_ready(namespace).await.unwrap(),
        Some(hash_2)
    );
    assert_eq!(
        store.take_next_ready(namespace).await.unwrap(),
        Some(hash_1)
    );
    assert_eq!(
        OrdererStore::<Hash>::take_next_ready(&store, namespace)
            .await
            .unwrap(),
        None
    );

    store.commit(permit).await.unwrap();
}

#[tokio::test]
async fn pending() {
    let store = SqliteStore::temporary().await;

    let namespace = "default";

    let hash_1 = Hash::digest(b"piff");
    let hash_2 = Hash::digest(b"puff");
    let hash_3 = Hash::digest(b"paff");
    let hash_4 = Hash::digest(b"peff");

    let permit = store.begin().await.unwrap();

    // 1. Should correctly return true or false when insertion occured.
    assert!(
        store
            .mark_pending(namespace, hash_1, vec![hash_2, hash_3])
            .await
            .unwrap()
    );
    assert!(
        store
            .mark_pending(namespace, hash_1, vec![hash_3])
            .await
            .unwrap()
    );
    assert!(
        !store
            .mark_pending(namespace, hash_1, vec![hash_3])
            .await
            .unwrap()
    );
    assert!(
        store
            .mark_pending(namespace, hash_1, vec![hash_4, hash_3])
            .await
            .unwrap()
    );

    // 2. Return correct list of pending items.
    let pending = store
        .get_next_pending(namespace, hash_2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.len(), 1);
    let (parent, deps) = pending.iter().next().unwrap();
    assert_eq!(*parent, hash_1);
    assert!(deps.contains(&hash_2));
    assert!(deps.contains(&hash_3));

    store.commit(permit).await.unwrap();
}

#[tokio::test]
async fn namespaces() {
    let store = SqliteStore::temporary().await;

    let namespace_1 = "ett";
    let namespace_2 = "två";

    let hash_1 = Hash::digest(b"eins");
    let hash_2 = Hash::digest(b"zwei");

    let permit = store.begin().await.unwrap();

    // Populate first namespace.
    assert!(store.mark_ready(namespace_1, hash_1).await.unwrap());
    assert!(store.mark_ready(namespace_1, hash_2).await.unwrap());

    // Populate second namespace.
    assert!(store.mark_ready(namespace_2, hash_1).await.unwrap());
    assert!(store.mark_ready(namespace_2, hash_2).await.unwrap());

    // Take next ready item from each namespace.
    assert_eq!(
        store.take_next_ready(namespace_1).await.unwrap(),
        Some(hash_1)
    );
    assert_eq!(
        store.take_next_ready(namespace_2).await.unwrap(),
        Some(hash_1)
    );

    store.commit(permit).await.unwrap();
}
