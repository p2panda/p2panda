// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use futures_test::task::noop_context;
use futures_util::TryStreamExt;
use tokio::time::sleep;
use tokio_stream::StreamExt;

use crate::address_book::AddressBook;
use crate::gossip::{Gossip, GossipEvent};
use crate::iroh_endpoint::Endpoint;
use crate::test_utils::{setup_logging, test_args};

#[tokio::test]
async fn joined_and_left_events_are_received() {
    setup_logging();
    let (mut ant_args, _) = test_args();
    let (mut bat_args, _) = test_args();
    let topic = [1; 32];

    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();

    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .config(bat_args.iroh_config.clone())
        .private_key(bat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    let ant_info = ant_args.node_info().bootstrap();
    let bat_info = bat_args.node_info().bootstrap();

    bat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();
    bat_address_book.insert_node_info(ant_info).await.unwrap();

    ant_address_book
        .set_topics(bat_info.node_id, [topic])
        .await
        .unwrap();
    ant_address_book.insert_node_info(bat_info).await.unwrap();

    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let bat_gossip = Gossip::builder(bat_address_book.clone(), bat_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Create the events subscriber channel _before_ creating the topic stream.
    //
    // This is to ensure we don't miss any events; this can happen, for example,
    // if the `Joined` event is sent before we have created the events
    // subscriber channel. In that case the `Joined` event will never be
    // received.
    let mut events = ant_gossip.events().await.unwrap();

    let ant_handle = ant_gossip.stream(topic).await.unwrap();
    let _bat_handle = bat_gossip.stream(topic).await.unwrap();

    // Gossip joined event is received by ant.
    assert!(matches!(
        events.recv().await,
        Ok(GossipEvent::Joined { .. })
    ));

    drop(ant_handle);

    // Gossip left event is received by ant.
    assert!(matches!(events.recv().await, Ok(GossipEvent::Left { .. })));
}

#[tokio::test]
async fn join_without_bootstrap() {
    setup_logging();

    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap node
    // - Cat joins the gossip topic using ant as bootstrap node
    //
    // Assert: Ant's gossip state includes the topic that was subscribed to
    // Assert: Ant's gossip state maps the subscribed topic to the public keys of bat and cat
    // (neighbours)

    let (mut ant_args, _) = test_args();
    let (bat_args, _) = test_args();
    let (cat_args, _) = test_args();

    let topic = [1; 32];

    // Create address books.
    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();
    let cat_address_book = AddressBook::builder().spawn().await.unwrap();

    // Create endpoints.
    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .config(bat_args.iroh_config.clone())
        .private_key(bat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let cat_endpoint = Endpoint::builder(cat_address_book.clone())
        .config(cat_args.iroh_config.clone())
        .private_key(cat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // Obtain ant's node information including direct addresses.
    let ant_info = ant_args.node_info().bootstrap();

    // Bat & Cat discovers ant through some out-of-band process. Note that Ant does _not_ have a
    // bootstrap specified.
    bat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    bat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();
    cat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    cat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();

    // Spawn gossip.
    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let bat_gossip = Gossip::builder(bat_address_book.clone(), bat_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let cat_gossip = Gossip::builder(cat_address_book.clone(), cat_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    let mut events = ant_gossip.events().await.unwrap();

    // Subscribe to gossip topic.
    let _ant_to_gossip = ant_gossip.stream(topic).await.unwrap();
    let _bat_to_gossip = bat_gossip.stream(topic).await.unwrap();
    let _cat_to_gossip = cat_gossip.stream(topic).await.unwrap();

    // Ant should have joined the overlay and learned about two more nodes.
    let mut neighbours = HashSet::new();

    if let GossipEvent::Joined {
        topic: event_topic,
        nodes,
    } = events.recv().await.unwrap()
    {
        assert_eq!(event_topic, topic);
        neighbours.extend(nodes);
    }

    if let GossipEvent::NeighbourUp {
        topic: event_topic,
        node,
    } = events.recv().await.unwrap()
    {
        assert_eq!(event_topic, topic);
        neighbours.insert(node);
    }

    assert_eq!(
        neighbours,
        HashSet::from([bat_args.public_key, cat_args.public_key])
    );
}

#[tokio::test]
async fn two_peer_gossip() {
    setup_logging();

    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap node
    //
    // Assert: Ant and bat can exchange messages

    let (mut ant_args, _) = test_args();
    let (bat_args, _) = test_args();

    let topic = [7; 32];

    // Create address books.
    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();

    // Create endpoints.
    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .config(bat_args.iroh_config.clone())
        .private_key(bat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // Obtain ant's node information including direct addresses.
    let ant_info = ant_args.node_info().bootstrap();

    // Bat discovers ant through some out-of-band process.
    bat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    bat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();

    // Spawn gossip.
    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let bat_gossip = Gossip::builder(bat_address_book.clone(), bat_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Subscribe to gossip topic.
    let ant_handle = ant_gossip.stream(topic).await.unwrap();
    let bat_handle = bat_gossip.stream(topic).await.unwrap();

    // Send message from ant to bat.
    ant_handle.publish(b"hi, bat!").await.unwrap();

    // Ensure bat receives the message from ant.
    let mut bat_from_gossip_rx = bat_handle.subscribe();
    let Some(Ok(msg)) = bat_from_gossip_rx.next().await else {
        panic!("expected msg from ant")
    };
    assert_eq!(msg, b"hi, bat!".to_vec());

    // Send message from bat to ant.
    bat_handle.publish(b"oh hey ant!").await.unwrap();

    // Ensure ant receives the message from bat.
    let mut ant_from_gossip_rx = ant_handle.subscribe();
    let Some(Ok(msg)) = ant_from_gossip_rx.next().await else {
        panic!("expected msg from bat")
    };
    assert_eq!(msg, b"oh hey ant!".to_vec());
}

#[ignore = "flaky"]
#[tokio::test]
async fn third_peer_joins_non_bootstrap() {
    setup_logging();

    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap node
    // - Cat joins the gossip topic using bat as bootstrap node
    //
    // Assert: Ant, bat and cat can exchange messages

    let (mut ant_args, _) = test_args();
    let (mut bat_args, _) = test_args();
    let (cat_args, _) = test_args();

    let topic = [11; 32];

    // Create address books.
    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();
    let cat_address_book = AddressBook::builder().spawn().await.unwrap();

    // Create endpoints.
    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .config(bat_args.iroh_config.clone())
        .private_key(bat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let cat_endpoint = Endpoint::builder(cat_address_book.clone())
        .config(cat_args.iroh_config.clone())
        .private_key(cat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // Obtain ant's node information including direct addresses.
    let ant_info = ant_args.node_info().bootstrap();

    // Bat discovers ant through some out-of-band process.
    bat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    bat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();

    // Spawn gossip.
    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let bat_gossip = Gossip::builder(bat_address_book.clone(), bat_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let cat_gossip = Gossip::builder(cat_address_book.clone(), cat_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Subscribe to gossip topic.
    let ant_handle = ant_gossip.stream(topic).await.unwrap();
    let bat_handle = bat_gossip.stream(topic).await.unwrap();

    let mut bat_from_gossip_rx = bat_handle.subscribe();

    // Obtain bat's endpoint information including direct addresses.
    let bat_info = bat_args.node_info().bootstrap();

    cat_address_book
        .insert_node_info(bat_info.clone())
        .await
        .unwrap();
    cat_address_book
        .set_topics(bat_info.node_id, [topic])
        .await
        .unwrap();

    // Cat subscribes to gossip overlay.
    let cat_handle = cat_gossip.stream(topic).await.unwrap();
    let mut cat_from_gossip_rx = cat_handle.subscribe();

    // Briefly sleep to allow overlay to form.
    sleep(Duration::from_millis(250)).await;

    // Send message from cat to ant and bat.
    let cat_msg_to_ant_and_bat = b"hi ant and bat!".to_vec();
    cat_handle
        .publish(cat_msg_to_ant_and_bat.clone())
        .await
        .unwrap();

    // Ensure bat receives cat's message.
    let Some(Ok(msg)) = bat_from_gossip_rx.next().await else {
        panic!("expected msg from cat")
    };
    assert_eq!(msg, cat_msg_to_ant_and_bat);

    // Send message from ant to bat and cat.
    let ant_msg_to_bat_and_cat = b"hi bat and cat!".to_vec();
    ant_handle
        .publish(ant_msg_to_bat_and_cat.clone())
        .await
        .unwrap();

    // Ensure cat receives ant's message.
    // NOTE: In this case the message is delivered by bat; not directly from ant.
    let Some(Ok(msg)) = cat_from_gossip_rx.next().await else {
        panic!("expected msg from ant")
    };
    assert_eq!(msg, ant_msg_to_bat_and_cat);
}

#[tokio::test]
async fn three_peer_gossip_with_rejoin() {
    setup_logging();

    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap node
    //
    // Assert: Ant and bat can exchange messages
    //
    // - Ant goes offline
    // - Cat joins the gossip topic using ant as bootstrap node
    //
    // Assert: Bat and cat can't exchange messages (proof of partition)
    //
    // - Cat learns about bat through out-of-band discovery process
    // - Cat joins bat on established gossip topic
    //
    // Assert: Bat and cat can now exchange messages (proof of healed partition)

    let (mut ant_args, _) = test_args();
    let (mut bat_args, _) = test_args();
    let (cat_args, _) = test_args();

    let topic = [9; 32];

    // Create address books.
    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();
    let cat_address_book = AddressBook::builder().spawn().await.unwrap();

    // Create endpoints.
    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .config(bat_args.iroh_config.clone())
        .private_key(bat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let cat_endpoint = Endpoint::builder(cat_address_book.clone())
        .config(cat_args.iroh_config.clone())
        .private_key(cat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // Obtain ant's node information including direct addresses.
    let ant_info = ant_args.node_info().bootstrap();

    // Bat discovers ant through some out-of-band process.
    bat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    bat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();

    // Spawn gossip.
    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let bat_gossip = Gossip::builder(bat_address_book.clone(), bat_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let cat_gossip = Gossip::builder(cat_address_book.clone(), cat_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Ant and bat subscribe to the gossip topic.
    let ant_handle = ant_gossip.stream(topic).await.unwrap();
    let bat_handle = bat_gossip.stream(topic).await.unwrap();

    let mut ant_from_gossip_rx = ant_handle.subscribe();
    let mut bat_from_gossip_rx = bat_handle.subscribe();

    // Send message from ant to bat.
    let ant_msg_to_bat = b"hi bat!".to_vec();
    ant_handle.publish(ant_msg_to_bat.clone()).await.unwrap();

    // Ensure bat receives the message from ant.
    let Some(Ok(msg)) = bat_from_gossip_rx.next().await else {
        panic!("expected msg from ant")
    };
    assert_eq!(msg, ant_msg_to_bat);

    // Send message from bat to ant.
    let bat_msg_to_ant = b"oh hey ant!".to_vec();
    bat_handle.publish(bat_msg_to_ant.clone()).await.unwrap();

    // Ensure ant receives the message from bat.
    let Some(Ok(msg)) = ant_from_gossip_rx.next().await else {
        panic!("expected msg from bat")
    };
    assert_eq!(msg, bat_msg_to_ant);

    // Ant is going offline.
    drop(ant_address_book);
    drop(ant_endpoint);

    // Cat joins the gossip topic (using ant as bootstrap).
    let cat_handle = cat_gossip.stream(topic).await.unwrap();
    let mut cat_from_gossip_rx = cat_handle.subscribe();

    // Send message from cat to bat.
    let cat_msg_to_bat = b"hi bat!".to_vec();
    cat_handle.publish(cat_msg_to_bat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(50)).await;

    // Ensure bat has not received the message from cat.
    let mut cx = noop_context();
    assert!(bat_from_gossip_rx.try_poll_next_unpin(&mut cx).is_pending());

    // Send message from bat to cat.
    let bat_msg_to_cat = b"anyone out there?".to_vec();
    bat_handle.publish(bat_msg_to_cat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(50)).await;

    // Ensure cat has not received the message from bat.
    let mut cx = noop_context();
    assert!(bat_from_gossip_rx.try_poll_next_unpin(&mut cx).is_pending());

    // At this point we have proof of partition; bat and cat are subscribed to the same gossip
    // topic but cannot "hear" one another.

    // Obtain bat's endpoint information including direct addresses.
    let bat_info = bat_args.node_info().bootstrap();

    // Cat discovers bat through some out-of-band process.
    cat_address_book
        .insert_node_info(bat_info.clone())
        .await
        .unwrap();
    cat_address_book
        .set_topics(bat_info.node_id, [topic])
        .await
        .unwrap();

    // Send message from cat to bat.
    let cat_msg_to_bat = b"you there bat?".to_vec();
    cat_handle.publish(cat_msg_to_bat.clone()).await.unwrap();

    // Ensure bat receives the message from cat.
    let Some(Ok(msg)) = bat_from_gossip_rx.next().await else {
        panic!("expected msg from cat")
    };

    assert_eq!(msg, cat_msg_to_bat);

    // Send message from bat to cat.
    let bat_msg_to_cat = b"yoyo!".to_vec();
    bat_handle.publish(bat_msg_to_cat.clone()).await.unwrap();

    // Ensure cat receives the message from bat.
    let Some(Ok(msg)) = cat_from_gossip_rx.next().await else {
        panic!("expected msg from bat")
    };
    assert_eq!(msg, bat_msg_to_cat);
}

#[tokio::test]
async fn leave_overlay_on_drop() {
    // See issue: https://github.com/p2panda/p2panda/issues/967
    setup_logging();

    let (mut ant_args, _) = test_args();
    let (mut bat_args, _) = test_args();
    let topic = [1; 32];

    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();

    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .config(bat_args.iroh_config.clone())
        .private_key(bat_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    let ant_info = ant_args.node_info().bootstrap();
    let bat_info = bat_args.node_info().bootstrap();

    bat_address_book
        .set_topics(ant_info.node_id, [topic])
        .await
        .unwrap();
    bat_address_book.insert_node_info(ant_info).await.unwrap();

    ant_address_book
        .set_topics(bat_info.node_id, [topic])
        .await
        .unwrap();
    ant_address_book.insert_node_info(bat_info).await.unwrap();

    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let bat_gossip = Gossip::builder(bat_address_book.clone(), bat_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    let ant_handle = ant_gossip.stream(topic).await.unwrap();
    let bat_handle = bat_gossip.stream(topic).await.unwrap();

    let mut bat_rx = bat_handle.subscribe();

    // 0. Check first if everything is working normally.
    // =================================================

    assert!(ant_handle.publish(b"test 0").await.is_ok());
    assert_eq!(bat_rx.next().await.unwrap().unwrap(), b"test 0".to_vec());

    // 1. Dropping all instances of Gossip breaks the handles.
    // =======================================================

    let ant_gossip_2 = ant_gossip.clone();

    // Drop second instance and assert if we can still send.
    drop(ant_gossip_2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(ant_handle.publish(b"test 1").await.is_ok());
    assert_eq!(bat_rx.next().await.unwrap().unwrap(), b"test 1".to_vec());

    // Drop first instance.
    drop(ant_gossip);

    // Wait a little bit until actor stopped.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // We should not be able anymore to send any messages into any handles.
    assert!(ant_handle.publish(b"test 1").await.is_err());

    // This handle is useless now, let's clean it up.
    drop(ant_handle);

    // 2. Dropping all handles for the same topic closes the gossip session.
    // =====================================================================

    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Subscribe to system events.
    let mut events = ant_gossip.events().await.unwrap();

    // Create multiple handles for the same topic.
    let ant_handle_1 = ant_gossip.stream(topic).await.unwrap();
    let ant_handle_2 = ant_gossip.stream(topic).await.unwrap();
    let ant_handle_3 = ant_gossip.stream(topic).await.unwrap();

    assert!(matches!(
        events.recv().await,
        Ok(GossipEvent::Joined { .. })
    ));

    let ant_rx_1 = ant_handle_1.subscribe();
    let ant_rx_2 = ant_handle_2.subscribe();
    let ant_rx_3 = ant_handle_2.subscribe();
    let ant_rx_4 = ant_handle_1.subscribe();

    // Start dropping instances, we should still be able to use the system.
    drop(ant_handle_2);
    drop(ant_rx_3);
    drop(ant_rx_2);

    assert!(ant_handle_1.publish(b"test 2").await.is_ok());
    assert_eq!(bat_rx.next().await.unwrap().unwrap(), b"test 2".to_vec());

    // Drop more.
    drop(ant_rx_1);
    drop(ant_handle_3);
    drop(ant_handle_1);

    // We haven't left the overlay yet.
    assert_eq!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    );

    // Finally drop the last instance referring to this topic.
    drop(ant_rx_4);

    // We expect to leave the overlay now for good.
    assert!(matches!(events.recv().await, Ok(GossipEvent::Left { .. })));

    // 3. Re-joining the same topic should not break anything after leaving.
    // =====================================================================

    // Make sure we've properly cleaned up internal state, so we can come back anytime.
    let handle = ant_gossip.stream(topic).await.unwrap();
    assert!(handle.publish(b"test 3").await.is_ok());
    assert_eq!(bat_rx.next().await.unwrap().unwrap(), b"test 3".to_vec());
}
