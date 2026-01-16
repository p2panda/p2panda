// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use assert_matches::assert_matches;
use p2panda_core::Operation;
use p2panda_sync::{FromSync, TopicLogSyncEvent as Event};
use tokio_stream::StreamExt;

use crate::test_utils::{TestNode, setup_logging};

#[tokio::test]
async fn e2e_log_sync() {
    setup_logging();

    let topic = [0; 32];
    let log_id = 0;

    let mut alice = TestNode::spawn([10; 32]).await;
    let mut bob = TestNode::spawn([11; 32]).await;

    alice
        .address_book
        .insert_node_info(bob.node_info())
        .await
        .unwrap();

    // Populate Alice's and Bob's store with some test data.
    alice
        .client
        .create_operation(b"Hello from Alice", log_id)
        .await;
    alice
        .client
        .insert_topic(&topic, HashMap::from([(alice.client_id(), vec![log_id])]))
        .await;

    bob.client.create_operation(b"Hello from Bob", log_id).await;
    bob.client
        .insert_topic(&topic, HashMap::from([(bob.client_id(), vec![log_id])]))
        .await;

    // Alice and Bob create stream for the same topic.
    let alice_handle = alice.log_sync.stream(topic, true).await.unwrap();
    let mut alice_subscription = alice_handle.subscribe().await.unwrap();

    let bob_handle = bob.log_sync.stream(topic, true).await.unwrap();
    let mut bob_subscription = bob_handle.subscribe().await.unwrap();

    // Alice manually initiates a sync session with Bob.
    alice_handle.initiate_session(bob.node_id());

    // Assert Alice receives the expected events.
    let bob_id = bob.node_id();
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        }) if remote == bob_id
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncFinished(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeStarted,
            ..
        })
    );

    // Assert Bob receives the expected events.
    let alice_id = alice.node_id();
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        }) if remote == alice_id
    );

    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncFinished(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeStarted,
            ..
        })
    );

    // Alice publishes "live" message.
    let (header, _, body) = alice
        .client
        .create_operation(b"live message from Alice", log_id)
        .await;
    alice_handle
        .publish(Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        })
        .await
        .unwrap();

    // Bob receives Alice's message.
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );

    // Drop Alice's stream to enforce closing live session with Bob.
    drop(alice_handle);

    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeFinished(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Success,
            ..
        })
    );

    // Assert Alice's final events.
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeFinished(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Success,
            ..
        })
    );
}

#[tokio::test]
async fn e2e_three_party_sync() {
    setup_logging();

    let topic = [0; 32];
    let log_id = 0;

    // Spawn nodes.
    let mut bob = TestNode::spawn([30; 32]).await;
    let mut alice = TestNode::spawn([31; 32]).await;
    let mut carol = TestNode::spawn([32; 32]).await;

    alice
        .address_book
        .insert_node_info(bob.args.node_info())
        .await
        .unwrap();

    carol
        .address_book
        .insert_node_info(alice.args.node_info())
        .await
        .unwrap();

    // Populate stores with some test data.
    alice
        .client
        .create_operation(b"Hello from Alice", log_id)
        .await;
    alice
        .client
        .insert_topic(&topic, HashMap::from([(alice.client_id(), vec![log_id])]))
        .await;

    bob.client.create_operation(b"Hello from Bob", log_id).await;
    bob.client
        .insert_topic(&topic, HashMap::from([(bob.client_id(), vec![log_id])]))
        .await;

    carol
        .client
        .create_operation(b"Hello from Carol", log_id)
        .await;
    carol
        .client
        .insert_topic(&topic, HashMap::from([(carol.client_id(), vec![log_id])]))
        .await;

    // Alice and Bob create stream for the same topic. Carol is inactive here.
    let alice_handle = alice.log_sync.stream(topic, true).await.unwrap();
    let mut alice_subscription = alice_handle.subscribe().await.unwrap();

    let bob_handle = bob.log_sync.stream(topic, true).await.unwrap();
    let mut bob_subscription = bob_handle.subscribe().await.unwrap();

    // Alice initiates sync.
    alice_handle.initiate_session(bob.node_id());

    // Assert Alice receives the expected events.
    let bob_id = bob.node_id();
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        }) if remote == bob_id
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncFinished(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeStarted,
            ..
        })
    );

    // Assert Bob receives the expected events.
    let alice_id = alice.node_id();
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        }) if remote == alice_id
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncFinished(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeStarted,
            ..
        })
    );

    // Alice publishes a live mode message.
    let (header, _, body) = alice
        .client
        .create_operation(b"live message from Alice", log_id)
        .await;
    alice_handle
        .publish(Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        })
        .await
        .unwrap();

    // Bob receives Alice's message.
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );

    // Create Carol's stream.
    let carol_handle = carol.log_sync.stream(topic, true).await.unwrap();
    let mut carol_subscription = carol_handle.subscribe().await.unwrap();

    // Carol initiates sync with Alice.
    carol_handle.initiate_session(alice.node_id());

    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            session_id: 0,
            event: Event::SyncStarted(_),
            ..
        })
    );
    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStatus(_),
            ..
        })
    );
    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );
    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Operation(_),
            ..
        })
    );
    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncFinished(_),
            ..
        })
    );
    let event = carol_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeStarted,
            ..
        })
    );
}

#[tokio::test]
async fn topic_log_sync_failure_and_retry() {
    setup_logging();

    let topic = [0; 32];
    let log_id = 0;

    let mut alice = TestNode::spawn([102; 32]).await;
    let mut bob = TestNode::spawn([103; 32]).await;

    bob.address_book
        .insert_node_info(alice.args.node_info())
        .await
        .unwrap();

    // Populate Alice's and Bob's store with some test data.
    alice
        .client
        .create_operation(b"Hello from Alice", log_id)
        .await;
    alice
        .client
        .insert_topic(&topic, HashMap::from([(alice.client_id(), vec![log_id])]))
        .await;

    bob.client.create_operation(b"Hello from Bob", log_id).await;
    bob.client
        .insert_topic(&topic, HashMap::from([(bob.client_id(), vec![log_id])]))
        .await;

    // Alice and Bob create stream for the same topic.
    let alice_handle = alice.log_sync.stream(topic, true).await.unwrap();
    let mut alice_subscription = alice_handle.subscribe().await.unwrap();

    let bob_handle = bob.log_sync.stream(topic, true).await.unwrap();
    let mut bob_subscription = bob_handle.subscribe().await.unwrap();

    // Bob manually initiates a sync session with Alice.
    bob_handle.initiate_session(alice.node_id());

    // Alice and Bob should receive all six events (SyncStarted, SyncStatus 2x, Operation,
    // SyncFinished and LiveModeStarted).
    for _ in 0..6 {
        alice_subscription.next().await.unwrap().unwrap();
    }

    for _ in 0..6 {
        bob_subscription.next().await.unwrap().unwrap();
    }

    // Alice unexpectedly shuts down.
    drop(alice);

    // Bob is informed that the session failed.
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::LiveModeFinished(_),
            ..
        })
    );
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::Failed { .. },
            ..
        })
    );

    // Alice starts up their node again and subscribes to the same topic.
    let alice = TestNode::spawn([102; 32]).await;
    alice
        .address_book
        .insert_node_info(bob.args.node_info())
        .await
        .unwrap();

    let alice_handle = alice.log_sync.stream(topic, true).await.unwrap();
    let mut alice_subscription = alice_handle.subscribe().await.unwrap();

    // Bob should automatically attempt restart and therefore both peers get a "sync started"
    // event.
    let event = bob_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStarted(_),
            ..
        })
    );
    let event = alice_subscription.next().await.unwrap();
    assert_matches!(
        event,
        Ok(FromSync {
            event: Event::SyncStarted(_),
            ..
        })
    );
}

use crate::sync::actors::manager::{GOSSIP_TOPIC_MIX_VALUE, derive_topic};

#[tokio::test]
async fn non_mix_topic_address_book_registration() {
    setup_logging();

    // We're running the same topic for both gossip and sync sessions, and even though they are the
    // same, they should be correctly treating different parts of the system.
    let topic = [1; 32];

    // Start Panda's node.
    let mut panda = TestNode::spawn([98; 32]).await;

    let mut panda_raw_topic_watcher_rx = panda.address_book.watch_topic(topic, true).await.unwrap();
    let mixed_topic = derive_topic(topic, GOSSIP_TOPIC_MIX_VALUE);
    let mut panda_mixed_topic_watcher_rx = panda
        .address_book
        .watch_topic(mixed_topic, true)
        .await
        .unwrap();

    // Subscribe to sync topic to receive (eventually consistent) messages.
    let panda_sync_handle = panda.log_sync.stream(topic, true).await.unwrap();
    let panda_sync_rx = panda_sync_handle.subscribe().await.unwrap();

    // Start Penguin's node.
    let mut penguin = TestNode::spawn([99; 32]).await;

    // Penguin adds Panda as a "bootstrap" node in its address book.
    penguin
        .address_book
        .insert_node_info(panda.node_info().bootstrap())
        .await
        .unwrap();

    let mut penguin_raw_topic_watcher_rx =
        panda.address_book.watch_topic(topic, true).await.unwrap();
    let mixed_topic = derive_topic(topic, GOSSIP_TOPIC_MIX_VALUE);
    let mut penguin_mixed_topic_watcher_rx = panda
        .address_book
        .watch_topic(mixed_topic, true)
        .await
        .unwrap();

    // Penguin initiates a sync stream for this topic and is ready now to share it's created
    // operation with Panda.
    let _penguin_sync_handle = penguin.log_sync.stream(topic, true).await.unwrap();

    // Assert panda and penguin each know they are both in the topic overlay for the "mixed" topic id.
    let event = panda_mixed_topic_watcher_rx.recv().await.unwrap();
    let node_ids = event.value;
    assert!(node_ids.contains(&panda.node_id()), "{node_ids:?}");
    assert!(node_ids.contains(&penguin.node_id()), "{node_ids:?}");

    let event = penguin_mixed_topic_watcher_rx.recv().await.unwrap();
    let node_ids = event.value;
    assert!(node_ids.contains(&panda.node_id()), "{node_ids:?}");
    assert!(node_ids.contains(&penguin.node_id()), "{node_ids:?}");

    // Both panda and penguin are also subscribed to the "raw" topic id gossip overlay.
    // TODO: this should not be the case. 
    let event = panda_raw_topic_watcher_rx.recv().await.unwrap();
    let node_ids = event.value;
    assert!(node_ids.contains(&panda.node_id()), "{node_ids:?}");
    assert!(node_ids.contains(&penguin.node_id()), "{node_ids:?}");

    let event = penguin_raw_topic_watcher_rx.recv().await.unwrap();
    let node_ids = event.value;
    assert!(node_ids.contains(&panda.node_id()), "{node_ids:?}");
    assert!(node_ids.contains(&penguin.node_id()), "{node_ids:?}");
}
