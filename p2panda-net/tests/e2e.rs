// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use futures_util::StreamExt;
use p2panda_core::Body;
use p2panda_net::test_utils::{TestNode, setup_logging};
use p2panda_sync::protocols::TopicLogSyncEvent;

#[tokio::test]
async fn gossip_and_sync_with_same_topic() {
    setup_logging();

    // We're running the same topic for both gossip and sync sessions, and even though they are the
    // same, they should be correctly treating different parts of the system.
    let topic = [1; 32];

    // ฅ՞•ﻌ•՞ฅ <- Panda
    // =======

    // Start Panda's node.
    let mut panda = TestNode::spawn([98; 32]).await;

    // Subscribe to gossip overlay to receive (ephemeral) messages.
    let panda_gossip_handle = panda.gossip.stream(topic).await.unwrap();
    let mut panda_gossip_rx = panda_gossip_handle.subscribe();

    // Panda waits for Penguin to send something.
    let panda_gossip_task = tokio::spawn(async move {
        while let Some(Ok(bytes)) = panda_gossip_rx.next().await {
            return Some(bytes);
        }
        return None;
    });

    // Subscribe to sync topic to receive (eventually consistent) messages.
    let panda_sync_handle = panda.log_sync.stream(topic, true).await.unwrap();
    let mut panda_sync_rx = panda_sync_handle.subscribe().await.unwrap();

    // Panda waits for Penguin to send something here as well.
    let panda_sync_task = tokio::spawn(async move {
        while let Some(Ok(item)) = panda_sync_rx.next().await {
            if let TopicLogSyncEvent::Operation(operation) = item.event {
                return Some(operation);
            }
        }
        return None;
    });

    // ૮(•͈⌔•͈)ა <- Penguin
    // =======

    // Start Penguin's node.
    let mut penguin = TestNode::spawn([99; 32]).await;

    // Penguin adds Panda as a "bootstrap" node in its address book.
    penguin
        .address_book
        .insert_node_info(panda.node_info().bootstrap())
        .await
        .unwrap();

    // Penguin publishes into the gossip overlay, so Panda can receive it.
    let penguin_gossip_handle = penguin.gossip.stream(topic).await.unwrap();
    penguin_gossip_handle
        .publish(b"Hello, Panda!")
        .await
        .unwrap();

    // Penguin stores an operation in the store, the sync protocol will pick it up.
    let log_id = 0;
    penguin
        .client
        .create_operation(b"Hello, again, Panda!", log_id)
        .await;
    penguin
        .client
        .insert_topic(&topic, HashMap::from([(penguin.client_id(), vec![log_id])]))
        .await;

    // Penguin initiates a sync stream for this topic and is ready now to share it's created
    // operation with Panda.
    let _penguin_sync_handle = penguin.log_sync.stream(topic, true).await.unwrap();

    // Wait until Panda receives something ..
    let message = panda_gossip_task.await.unwrap();
    assert_eq!(message, Some(b"Hello, Panda!".to_vec()));

    let operation = panda_sync_task.await.unwrap().unwrap();
    assert_eq!(operation.body, Some(Body::new(b"Hello, again, Panda!")));
}
