// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use assert_matches::assert_matches;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::{Endpoint, protocol::Router};
use p2panda_core::Operation;
use p2panda_net::cbor::{into_cbor_sink, into_cbor_stream};
use p2panda_sync::FromSync;
use p2panda_sync::protocols::{Logs, TopicLogSyncEvent as Event};
use p2panda_sync::test_utils::{Peer, TestTopic, TestTopicSyncMessage};
use p2panda_sync::traits::Protocol;
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
async fn unsubscribe_from_gossip_after_drop() {
    setup_logging();

    let sync_topic = [0; 32];

    let alice = TestNode::spawn([73; 32]).await;
    let alice_handle = alice.log_sync.stream(sync_topic, true).await.unwrap();

    let mut watcher = alice
        .address_book
        .watch_node_topics(alice.node_id(), false)
        .await
        .unwrap();

    // Alice should be subscribed to the topic.
    while let Some(event) = watcher.recv().await {
        // Assert that the original sync topic is _not_ used but the derived gossip topic instead.
        if !event.value.contains(&sync_topic) && event.value.len() == 1 {
            break;
        }
    }

    // Alice should be unsubscribed from the topic after dropping the sync handle.
    drop(alice_handle);

    while let Some(event) = watcher.recv().await {
        if event.value.is_empty() {
            break;
        }
    }
}

const ALPN: &[u8] = b"iroh/smol/0";

#[derive(Debug, Clone, Default)]
struct TestProtocol {}

impl ProtocolHandler for TestProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let _ = connection.accept_bi().await;
        // No need to do anything else here as we expect the connection to immediately close.
        Ok(())
    }

    async fn shutdown(&self) {}
}

#[tokio::test]
async fn panic_on_sink_closure_after_error_regression() {
    // This is a regression test for an issue where chaining adaptors on the message sink in
    // TopicLogSync was causing a panic under certain error conditions:
    // https://github.com/p2panda/p2panda/issues/970
    //
    // The issue could only be reproduced when using an actual QUIC stream as the underlying
    // transport. Here we use a connection between two iroh endpoints.
    setup_logging();

    let topic = TestTopic::new("messages");
    let mut peer = Peer::new(0);
    peer.insert_topic(&topic, &Logs::default());

    let (session, _events_rx, _live_tx) = peer.topic_sync_protocol(topic.clone(), true);

    let acceptor = Endpoint::bind().await.unwrap();
    let acceptor_router = Router::builder(acceptor)
        .accept(ALPN, TestProtocol::default())
        .spawn();
    let addr = acceptor_router.endpoint().addr();

    let initiator = Endpoint::bind().await.unwrap();
    let connection = initiator.connect(addr, ALPN).await.unwrap();
    let (tx, rx) = connection.open_bi().await.unwrap();
    let mut tx = into_cbor_sink::<TestTopicSyncMessage, _>(tx);
    let mut rx = into_cbor_stream::<TestTopicSyncMessage, _>(rx);

    let handle = tokio::spawn(async move { session.run(&mut tx, &mut rx).await });

    // Unexpectedly closing the connection here on the "initiator" side causes the initial sync
    // protocol (before live-mode) to end with an error. After the error is correctly handled
    // sink.close() is called and _this_ causes a panic in the underlying message sink due to the
    // way it was wrapped in both a .with() and .sink_map_err() adaptor. The panic is caused
    // because both these wrappers end up calling poll_close() and doing this after the sink is
    // already in a closed state causes an error. The fix is to introduce a custom Sink wrapper
    // instead of chaining adaptors.
    connection.close(0u32.into(), b"testing");

    let result = handle.await.unwrap();
    assert!(result.is_err());
}
