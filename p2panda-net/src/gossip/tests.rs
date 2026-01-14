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

    // Subscribe to gossip topic.
    let _ant_to_gossip = ant_gossip.stream(topic).await.unwrap();
    let _bat_to_gossip = bat_gossip.stream(topic).await.unwrap();
    let _cat_to_gossip = cat_gossip.stream(topic).await.unwrap();

    // Ant should have joined the overlay and learned about two more nodes.
    let mut neighbours = HashSet::new();
    let mut events = ant_gossip.events().await.unwrap();

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

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(50)).await;

    // Ensure bat receives the message from cat.
    let Some(Ok(msg)) = bat_from_gossip_rx.next().await else {
        panic!("expected msg from cat")
    };

    assert_eq!(msg, cat_msg_to_bat);

    // Send message from bat to cat.
    let bat_msg_to_cat = b"yoyo!".to_vec();
    bat_handle.publish(bat_msg_to_cat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(500)).await;

    // Ensure cat receives the message from bat.
    let Some(Ok(msg)) = cat_from_gossip_rx.next().await else {
        panic!("expected msg from bat")
    };
    assert_eq!(msg, bat_msg_to_cat);
}

#[tokio::test]
async fn leave_session() {
    setup_logging();

    let (ant_args, _) = test_args();
    let topic = [1; 32];

    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .config(ant_args.iroh_config.clone())
        .private_key(ant_args.private_key.clone())
        .spawn()
        .await
        .unwrap();

    // 1. Dropping all instances of Gossip breaks the handles.
    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();
    let ant_gossip_2 = ant_gossip.clone();
    let handle = ant_gossip.stream(topic).await.unwrap();

    // Drop first instance and assert if we can still send.
    drop(ant_gossip_2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(handle.publish(b"test").await.is_ok());

    drop(ant_gossip);

    // Wait a little bit until actor stopped.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // We should not be able anymore to send any messages into any handles.
    assert!(handle.publish(b"test").await.is_err());

    // 2. Dropping all handles for the same topic closes the gossip session.
    let ant_gossip = Gossip::builder(ant_address_book.clone(), ant_endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Subscribe to system events.
    let mut events = ant_gossip.events().await.unwrap();

    // Create multiple handles for the same topic.
    let handle_1 = ant_gossip.stream(topic).await.unwrap();
    let handle_2 = ant_gossip.stream(topic).await.unwrap();
    let handle_3 = ant_gossip.stream(topic).await.unwrap();

    let rx_1 = handle_1.subscribe();
    let rx_2 = handle_2.subscribe();
    let rx_3 = handle_2.subscribe();
    let rx_4 = handle_1.subscribe();

    // Start dropping instances, we should still be able to use the system.
    drop(handle_2);
    drop(rx_3);
    drop(rx_2);

    assert!(handle_1.publish(b"test").await.is_ok());

    // Drop more.
    drop(rx_1);
    drop(handle_3);
    drop(handle_1);
    drop(rx_4);

    let mut left = false;
    while let Ok(event) = events.recv().await {
        if let GossipEvent::Left { topic: event_topic } = event {
            assert_eq!(topic, event_topic);
            left = true;
            break;
        }
    }
    assert!(left, "left the gossip overlay for this topic");

    // 3. Re-joining the same topic should not break anything after leaving.
    let handle = ant_gossip.stream(topic).await.unwrap();
    assert!(handle.publish(b"test").await.is_ok());
}
