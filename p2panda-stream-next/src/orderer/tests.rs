// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

// @TODO: Change this to p2panda_store when ready.
use p2panda_store_next::orderer::OrdererTestExt;
use p2panda_store_next::sqlite::SqliteStoreBuilder;

use crate::orderer::CausalOrderer;

#[tokio::test]
async fn partial_order() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    // Graph
    //
    // A <-- B <--------- D
    //        \--- C <---/
    //
    let graph = [
        ("A".to_string(), vec![]),
        ("B".to_string(), vec!["A".to_string()]),
        ("C".to_string(), vec!["B".to_string()]),
        ("D".to_string(), vec!["B".to_string(), "C".to_string()]),
    ];

    // A has no dependencies and so it's added straight to the processed set and ready
    // queue.

    let mut orderer = CausalOrderer::new(store);
    let item = graph[0].clone();
    orderer.process(item.0, &item.1).await.unwrap();
    assert_eq!(orderer.store.ready_len().await, 1);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 1);

    // B has it's dependencies met and so it too is added to the processed set and ready
    // queue.
    let item = graph[1].clone();
    orderer.process(item.0, &item.1).await.unwrap();
    assert_eq!(orderer.store.ready_len().await, 2);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 2);

    // D doesn't have both its dependencies met yet so it waits in the pending queue.
    let item = graph[3].clone();
    orderer.process(item.0, &item.1).await.unwrap();
    assert_eq!(orderer.store.ready_len().await, 2);
    assert_eq!(orderer.store.pending_len().await, 1);
    assert_eq!(orderer.store.ready_queue_len().await, 2);

    // C satisfies D's dependencies and so both C & D are added to the processed set
    // and ready queue.
    let item = graph[2].clone();
    orderer.process(item.0, &item.1).await.unwrap();
    assert_eq!(orderer.store.ready_len().await, 4);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 4);

    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("A".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("B".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("C".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("D".to_string()));
    let item = orderer.next().await.unwrap();
    assert!(item.is_none());
}

#[tokio::test]
async fn idempotency() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    // Graph
    //
    // A <-- B
    //
    let graph = [
        ("A".to_string(), vec![]),
        ("B".to_string(), vec!["A".to_string()]),
    ];

    let mut orderer = CausalOrderer::new(store);

    let item_a = graph[0].clone();
    let item_b = graph[1].clone();

    orderer.process(item_b.clone().0, &item_b.1).await.unwrap();

    // No dependencies met yet.
    assert!(orderer.next().await.unwrap().is_none());

    // A and B is ready now after processing A.
    orderer.process(item_a.clone().0, &item_a.1).await.unwrap();
    assert_eq!(orderer.next().await.unwrap(), Some(item_a.0.clone()));
    assert_eq!(orderer.next().await.unwrap(), Some(item_b.0.clone()));

    assert_eq!(orderer.store.ready_len().await, 2);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 0);

    // Re-process B, it should just get forwarded without changes to the orderer state.
    orderer.process(item_b.clone().0, &item_b.1).await.unwrap();
    assert_eq!(orderer.next().await.unwrap(), Some(item_b.0.clone()));

    assert_eq!(orderer.store.ready_len().await, 2);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 0);
}

#[tokio::test]
async fn partial_order_with_recursion() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    // Graph
    //
    // A <-- B <--------- D
    //        \--- C <---/
    //
    let incomplete_graph = [
        ("A".to_string(), vec![]),
        ("C".to_string(), vec!["B".to_string()]),
        ("D".to_string(), vec!["C".to_string()]),
        ("E".to_string(), vec!["D".to_string()]),
        ("F".to_string(), vec!["E".to_string()]),
        ("G".to_string(), vec!["F".to_string()]),
    ];

    let mut orderer = CausalOrderer::new(store);
    for (key, dependencies) in incomplete_graph {
        orderer.process(key, &dependencies).await.unwrap();
    }
    assert_eq!(orderer.store.ready_len().await, 1);
    assert_eq!(orderer.store.pending_len().await, 5);
    assert_eq!(orderer.store.ready_queue_len().await, 1);

    let missing_dependency = ("B".to_string(), vec!["A".to_string()]);

    orderer
        .process(missing_dependency.0, &missing_dependency.1)
        .await
        .unwrap();
    assert_eq!(orderer.store.ready_len().await, 7);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 7);

    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("A".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("B".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("C".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("D".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("E".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("F".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("G".to_string()));
    let item = orderer.next().await.unwrap();
    assert!(item.is_none());
}

#[tokio::test]
async fn complex_graph() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    // Graph
    //
    // A <-- B1 <-- C1 <--\
    //   \-- ?? <-- C2 <-- D
    //        \---- C3 <--/
    //
    let incomplete_graph = [
        ("A".to_string(), vec![]),
        ("B1".to_string(), vec!["A".to_string()]),
        // This item is missing.
        // ("B2", vec!["A"]),
        ("C1".to_string(), vec!["B1".to_string()]),
        ("C2".to_string(), vec!["B2".to_string()]),
        ("C3".to_string(), vec!["B2".to_string()]),
        (
            "D".to_string(),
            vec!["C1".to_string(), "C2".to_string(), "C3".to_string()],
        ),
    ];

    let mut orderer = CausalOrderer::new(store);
    for (key, dependencies) in incomplete_graph {
        orderer.process(key, &dependencies).await.unwrap();
    }

    // A1, B1 and C1 have dependencies met and were already processed.
    assert!(orderer.store.ready_len().await == 3);
    assert_eq!(orderer.store.pending_len().await, 3);
    assert_eq!(orderer.store.ready_queue_len().await, 3);

    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("A".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("B1".to_string()));
    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("C1".to_string()));
    let item = orderer.next().await.unwrap();
    assert!(item.is_none());

    // No more ready items.
    assert_eq!(orderer.store.ready_queue_len().await, 0);

    // Process the missing item.
    let missing_dependency = ("B2".to_string(), vec!["A".to_string()]);
    orderer
        .process(missing_dependency.0, &missing_dependency.1)
        .await
        .unwrap();

    // All items have now been processed and new ones are waiting in the ready queue.
    assert_eq!(orderer.store.ready_len().await, 7);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 4);

    let mut concurrent_items = HashSet::from(["C2".to_string(), "C3".to_string()]);

    let item = orderer.next().await.unwrap().unwrap();
    assert_eq!(item, "B2".to_string());
    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert_eq!(item, "D".to_string());
    let item = orderer.next().await.unwrap();
    assert!(item.is_none());
}

#[tokio::test]
async fn very_out_of_order() {
    let store = SqliteStoreBuilder::new()
        .random_memory_url()
        .max_connections(1)
        .build()
        .await
        .unwrap();

    // Graph
    //
    // A <-- B1 <-- C1 <--\
    //   \-- B2 <-- C2 <-- D
    //        \---- C3 <--/
    //
    let out_of_order_graph = [
        (
            "D".to_string(),
            vec!["C1".to_string(), "C2".to_string(), "C3".to_string()],
        ),
        ("C1".to_string(), vec!["B1".to_string()]),
        ("B1".to_string(), vec!["A".to_string()]),
        ("B2".to_string(), vec!["A".to_string()]),
        ("C3".to_string(), vec!["B2".to_string()]),
        ("C2".to_string(), vec!["B2".to_string()]),
        ("A".to_string(), vec![]),
    ];

    let mut orderer = CausalOrderer::new(store);
    for (key, dependencies) in out_of_order_graph {
        orderer.process(key, &dependencies).await.unwrap();
    }

    assert_eq!(orderer.store.ready_len().await, 7);
    assert_eq!(orderer.store.pending_len().await, 0);
    assert_eq!(orderer.store.ready_queue_len().await, 7);

    let item = orderer.next().await.unwrap();
    assert_eq!(item, Some("A".to_string()));

    let mut concurrent_items = HashSet::from([
        "B1".to_string(),
        "B2".to_string(),
        "C1".to_string(),
        "C2".to_string(),
        "C3".to_string(),
    ]);

    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert!(concurrent_items.remove(&item));
    let item = orderer.next().await.unwrap().unwrap();
    assert_eq!(item, "D".to_string());
    let item = orderer.next().await.unwrap();
    assert!(item.is_none());
}
