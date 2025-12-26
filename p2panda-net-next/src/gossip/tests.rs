// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use iroh::discovery::EndpointInfo;
use iroh::discovery::static_provider::StaticProvider;
use iroh::protocol::Router as IrohRouter;
use iroh::{self, RelayMode};
use p2panda_core::PublicKey;
use p2panda_discovery::address_book::{AddressBookStore, NodeInfo as _};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::time::sleep;
use tracing::info;

use crate::address_book::AddressBook;
use crate::gossip::{Gossip, GossipEvent};
use crate::iroh::Endpoint;
use crate::test_utils::{generate_trusted_node_info, setup_logging, test_args};

#[tokio::test]
async fn join_without_bootstrap() {
    setup_logging();

    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap node
    // - Cat joins the gossip topic using ant as bootstrap node
    //
    // - Assert: Ant's gossip state includes the topic that was subscribed to
    // - Assert: Ant's gossip state maps the subscribed topic to the public keys of
    //           bat and cat (neighbours)

    let (mut ant_args, _, _) = test_args();
    let (bat_args, _, _) = test_args();
    let (cat_args, _, _) = test_args();

    let topic = [1; 32];

    // Create address books.
    let ant_address_book = AddressBook::builder().spawn().await.unwrap();
    let bat_address_book = AddressBook::builder().spawn().await.unwrap();
    let cat_address_book = AddressBook::builder().spawn().await.unwrap();

    // Create endpoints.
    let ant_endpoint = Endpoint::builder(ant_address_book.clone())
        .private_key(ant_args.private_key.clone())
        .config(ant_args.iroh_config.clone())
        .spawn()
        .await
        .unwrap();
    let bat_endpoint = Endpoint::builder(bat_address_book.clone())
        .private_key(bat_args.private_key.clone())
        .config(bat_args.iroh_config.clone())
        .spawn()
        .await
        .unwrap();
    let cat_endpoint = Endpoint::builder(cat_address_book.clone())
        .private_key(cat_args.private_key.clone())
        .config(cat_args.iroh_config.clone())
        .spawn()
        .await
        .unwrap();

    // Obtain ant's node information including direct addresses.
    let ant_info = generate_trusted_node_info(&mut ant_args).bootstrap();

    // Bat & Cat discovers ant through some out-of-band process. Note that Ant does _not_ have a
    // bootstrap specified.
    bat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    bat_address_book
        .set_ephemeral_messaging_topics(ant_info.node_id, [topic])
        .await
        .unwrap();
    cat_address_book
        .insert_node_info(ant_info.clone())
        .await
        .unwrap();
    cat_address_book
        .set_ephemeral_messaging_topics(ant_info.node_id, [topic])
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

// #[tokio::test]
// async fn two_peer_gossip() {
//     // Scenario:
//     //
//     // - Ant joins the gossip topic
//     // - Bat joins the gossip topic using ant as bootstrap node
//     // - Assert: Ant and bat can exchange messages
//
//     let (ant_args, ant_store, _) = test_args();
//     let (bat_args, bat_store, _) = test_args();
//
//     let mixed_alpn = hash_protocol_id_with_network_id(iroh_gossip::ALPN, ant_args.network_id);
//
//     let topic = [7; 32];
//
//     // Create keypairs.
//     let ant_private_key = ant_args.private_key.clone();
//     let bat_private_key = bat_args.private_key.clone();
//
//     let ant_public_key = ant_private_key.public_key();
//
//     // Create endpoints.
//     let ant_discovery = StaticProvider::new();
//     let ant_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(ant_private_key))
//         .discovery(ant_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     let bat_discovery = StaticProvider::new();
//     let bat_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(bat_private_key))
//         .discovery(bat_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     // Obtain ant's endpoint information including direct addresses.
//     let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();
//
//     // Bat discovers ant through some out-of-band process.
//     bat_discovery.add_endpoint_info(ant_endpoint_info);
//
//     let thread_pool = ThreadLocalActorSpawner::new();
//
//     // Spawn one address book for each node.
//     let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
//     let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
//
//     let (ant_address_book_actor, ant_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
//         (ant_args.clone(), ant_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (bat_address_book_actor, bat_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
//         (bat_args.clone(), bat_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//
//     // Spawn gossip actors.
//     let (ant_gossip_actor, ant_gossip_actor_handle) =
//         TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//     let (bat_gossip_actor, bat_gossip_actor_handle) =
//         TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//
//     // Get handles to gossip.
//     let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
//     let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
//
//     // Build and spawn routers.
//     let ant_router = IrohRouter::builder(ant_endpoint.clone())
//         .accept(&mixed_alpn, ant_gossip)
//         .spawn();
//     let bat_router = IrohRouter::builder(bat_endpoint.clone())
//         .accept(&mixed_alpn, bat_gossip)
//         .spawn();
//
//     // Subscribe to the gossip topic.
//     let ant_peers = Vec::new();
//     let bat_peers = vec![ant_public_key];
//
//     let (ant_to_gossip, ant_from_gossip) =
//         call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
//     let (bat_to_gossip, bat_from_gossip) =
//         call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();
//
//     // Briefly sleep to allow overlay to form.
//     sleep(Duration::from_millis(100)).await;
//
//     // Subscribe to sender to obtain receiver.
//     let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
//     let mut ant_from_gossip_rx = ant_from_gossip.subscribe();
//
//     // Send message from ant to bat.
//     let ant_msg_to_bat = b"hi bat!".to_vec();
//     ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();
//
//     // Ensure bat receives the message from ant.
//     let Ok(msg) = bat_from_gossip_rx.recv().await else {
//         panic!("expected msg from ant")
//     };
//
//     assert_eq!(msg, ant_msg_to_bat);
//
//     // Send message from bat to ant.
//     let bat_msg_to_ant = b"oh hey ant!".to_vec();
//     bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();
//
//     // Ensure ant receives the message from bat.
//     let Ok(msg) = ant_from_gossip_rx.recv().await else {
//         panic!("expected msg from bat")
//     };
//
//     assert_eq!(msg, bat_msg_to_ant);
//
//     // Stop gossip actors.
//     ant_gossip_actor.stop(None);
//     bat_gossip_actor.stop(None);
//     ant_gossip_actor_handle.await.unwrap();
//     bat_gossip_actor_handle.await.unwrap();
//
//     // Stop address book actors.
//     ant_address_book_actor.stop(None);
//     bat_address_book_actor.stop(None);
//     ant_address_book_actor_handle.await.unwrap();
//     bat_address_book_actor_handle.await.unwrap();
//
//     // Shutdown routers.
//     bat_router.shutdown().await.unwrap();
//     ant_router.shutdown().await.unwrap();
// }
//
// #[ignore = "flaky"]
// #[tokio::test]
// async fn third_peer_joins_non_bootstrap() {
//     // Scenario:
//     //
//     // - Ant joins the gossip topic
//     // - Bat joins the gossip topic using ant as bootstrap node
//     // - Cat joins the gossip topic using bat as bootstrap node
//     // - Assert: Ant, bat and cat can exchange messages
//
//     let (ant_args, ant_store, _) = test_args();
//     let (bat_args, bat_store, _) = test_args();
//     let (cat_args, cat_store, _) = test_args();
//
//     let mixed_alpn = hash_protocol_id_with_network_id(iroh_gossip::ALPN, &ant_args.network_id);
//
//     let topic = [11; 32];
//
//     // Create keypairs.
//     let ant_private_key = ant_args.private_key.clone();
//     let bat_private_key = bat_args.private_key.clone();
//     let cat_private_key = cat_args.private_key.clone();
//
//     let ant_public_key = ant_private_key.public_key();
//     let bat_public_key = bat_private_key.public_key();
//
//     // Create endpoints.
//     let ant_discovery = StaticProvider::new();
//     let ant_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(ant_private_key))
//         .discovery(ant_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     let bat_discovery = StaticProvider::new();
//     let bat_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(bat_private_key))
//         .discovery(bat_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     let cat_discovery = StaticProvider::new();
//     let cat_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(cat_private_key))
//         .discovery(cat_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     // Obtain ant's endpoint information including direct addresses.
//     let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();
//
//     // Bat discovers ant through some out-of-band process.
//     bat_discovery.add_endpoint_info(ant_endpoint_info);
//
//     let thread_pool = ThreadLocalActorSpawner::new();
//
//     let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
//     let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
//     let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);
//
//     let (ant_address_book_actor, ant_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
//         (ant_args.clone(), ant_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (bat_address_book_actor, bat_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
//         (bat_args.clone(), bat_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (cat_address_book_actor, cat_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
//         (cat_args.clone(), cat_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//
//     // Spawn gossip actors.
//     let (ant_gossip_actor, ant_gossip_actor_handle) =
//         TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//     let (bat_gossip_actor, bat_gossip_actor_handle) =
//         TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//     let (cat_gossip_actor, cat_gossip_actor_handle) =
//         TestGossip::spawn(None, (cat_args, cat_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//
//     // Get handles to gossip.
//     let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
//     let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
//     let cat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
//
//     // Build and spawn routers.
//     let ant_router = IrohRouter::builder(ant_endpoint.clone())
//         .accept(&mixed_alpn, ant_gossip)
//         .spawn();
//     let bat_router = IrohRouter::builder(bat_endpoint.clone())
//         .accept(&mixed_alpn, bat_gossip)
//         .spawn();
//     let cat_router = IrohRouter::builder(cat_endpoint.clone())
//         .accept(&mixed_alpn, cat_gossip)
//         .spawn();
//
//     // Subscribe to the gossip topic.
//     let ant_peers = Vec::new();
//     let bat_peers = vec![ant_public_key];
//
//     let (ant_to_gossip, _ant_from_gossip) =
//         call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
//     let (_bat_to_gossip, bat_from_gossip) =
//         call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();
//
//     // Briefly sleep to allow overlay to form.
//     sleep(Duration::from_millis(250)).await;
//
//     // Subscribe to sender to obtain receiver.
//     let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
//
//     // Obtain bat's endpoint information including direct addresses.
//     let bat_endpoint_info: EndpointInfo = bat_endpoint.addr().into();
//
//     // Cat discovers bat through some out-of-band process.
//     cat_discovery.add_endpoint_info(bat_endpoint_info);
//
//     let cat_peers = vec![bat_public_key];
//
//     // Cat subscribes to topic using bat as bootstrap.
//     let (cat_to_gossip, cat_from_gossip) =
//         call!(cat_gossip_actor, ToGossip::Subscribe, topic, cat_peers).unwrap();
//
//     // Briefly sleep to allow overlay to form.
//     sleep(Duration::from_millis(250)).await;
//
//     let mut cat_from_gossip_rx = cat_from_gossip.subscribe();
//
//     // Send message from cat to ant and bat.
//     let cat_msg_to_ant_and_bat = b"hi ant and bat!".to_vec();
//     cat_to_gossip
//         .send(cat_msg_to_ant_and_bat.clone())
//         .await
//         .unwrap();
//
//     // Ensure bat receives cat's message.
//     let Ok(msg) = bat_from_gossip_rx.recv().await else {
//         panic!("expected msg from cat")
//     };
//
//     assert_eq!(msg, cat_msg_to_ant_and_bat);
//
//     // Send message from ant to bat and cat.
//     let ant_msg_to_bat_and_cat = b"hi bat and cat!".to_vec();
//     ant_to_gossip
//         .send(ant_msg_to_bat_and_cat.clone())
//         .await
//         .unwrap();
//
//     // Ensure cat receives ant's message.
//     let Ok(msg) = cat_from_gossip_rx.recv().await else {
//         panic!("expected msg from ant")
//     };
//
//     // NOTE: In this case the message is delivered by bat; not directly from ant.
//     assert_eq!(msg, ant_msg_to_bat_and_cat);
//
//     // Stop gossip actors.
//     ant_gossip_actor.stop(None);
//     bat_gossip_actor.stop(None);
//     cat_gossip_actor.stop(None);
//     ant_gossip_actor_handle.await.unwrap();
//     bat_gossip_actor_handle.await.unwrap();
//     cat_gossip_actor_handle.await.unwrap();
//
//     // Stop address book actors.
//     ant_address_book_actor.stop(None);
//     bat_address_book_actor.stop(None);
//     cat_address_book_actor.stop(None);
//     ant_address_book_actor_handle.await.unwrap();
//     bat_address_book_actor_handle.await.unwrap();
//     cat_address_book_actor_handle.await.unwrap();
//
//     // Shutdown routers.
//     ant_router.shutdown().await.unwrap();
//     bat_router.shutdown().await.unwrap();
//     cat_router.shutdown().await.unwrap();
// }
//
// #[tokio::test]
// async fn three_peer_gossip_with_rejoin() {
//     // Scenario:
//     //
//     // - Ant joins the gossip topic
//     // - Bat joins the gossip topic using ant as bootstrap node
//     // - Assert: Ant and bat can exchange messages
//     // - Ant goes offline
//     // - Cat joins the gossip topic using ant as bootstrap node
//     // - Assert: Bat and cat can't exchange messages (proof of partition)
//     // - Cat learns about bat through out-of-band discovery process
//     // - Cat joins bat on established gossip topic
//     // - Assert: Bat and cat can now exchange messages (proof of healed partition)
//
//     let (ant_args, ant_store, _) = test_args();
//     let (bat_args, bat_store, _) = test_args();
//     let (cat_args, cat_store, _) = test_args();
//
//     let mixed_alpn = hash_protocol_id_with_network_id(iroh_gossip::ALPN, &ant_args.network_id);
//
//     let topic = [9; 32];
//
//     // Create keypairs.
//     let ant_private_key = ant_args.private_key.clone();
//     let bat_private_key = bat_args.private_key.clone();
//     let cat_private_key = cat_args.private_key.clone();
//
//     let ant_public_key = ant_private_key.public_key();
//     let bat_public_key = bat_private_key.public_key();
//
//     // Create endpoints.
//     let ant_discovery = StaticProvider::new();
//     let ant_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(ant_private_key))
//         .discovery(ant_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     let bat_discovery = StaticProvider::new();
//     let bat_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(bat_private_key))
//         .discovery(bat_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     let cat_discovery = StaticProvider::new();
//     let cat_endpoint = iroh::Endpoint::empty_builder(RelayMode::Disabled)
//         .secret_key(from_private_key(cat_private_key))
//         .discovery(cat_discovery.clone())
//         .bind()
//         .await
//         .unwrap();
//
//     // Obtain ant's endpoint information including direct addresses.
//     let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();
//
//     // Bat discovers ant through some out-of-band process.
//     bat_discovery.add_endpoint_info(ant_endpoint_info);
//
//     let thread_pool = ThreadLocalActorSpawner::new();
//
//     // Spawn one address book for each node.
//     let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
//     let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
//     let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);
//
//     let (ant_address_book_actor, ant_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
//         (ant_args.clone(), ant_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (bat_address_book_actor, bat_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
//         (bat_args.clone(), bat_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (cat_address_book_actor, cat_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
//         (cat_args.clone(), cat_store.clone()),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//
//     // Spawn gossip actors.
//     let (ant_gossip_actor, ant_gossip_actor_handle) =
//         TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//     let (bat_gossip_actor, bat_gossip_actor_handle) =
//         TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//     let (cat_gossip_actor, cat_gossip_actor_handle) =
//         TestGossip::spawn(None, (cat_args, cat_endpoint.clone()), thread_pool.clone())
//             .await
//             .unwrap();
//
//     // Get handles to gossip.
//     let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
//     let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
//     let cat_gossip = call!(cat_gossip_actor, ToGossip::Handle).unwrap();
//
//     // Build and spawn routers.
//     let ant_router = IrohRouter::builder(ant_endpoint.clone())
//         .accept(&mixed_alpn, ant_gossip)
//         .spawn();
//     let bat_router = IrohRouter::builder(bat_endpoint.clone())
//         .accept(&mixed_alpn, bat_gossip)
//         .spawn();
//     let cat_router = IrohRouter::builder(cat_endpoint.clone())
//         .accept(&mixed_alpn, cat_gossip)
//         .spawn();
//
//     // Ant and bat subscribe to the gossip topic.
//     let ant_peers = Vec::new();
//     let bat_peers = vec![ant_public_key];
//
//     let (ant_to_gossip, ant_from_gossip) =
//         call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
//     let (bat_to_gossip, bat_from_gossip) =
//         call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();
//
//     // Subscribe to sender to obtain receiver.
//     let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
//     let mut ant_from_gossip_rx = ant_from_gossip.subscribe();
//
//     // Send message from ant to bat.
//     let ant_msg_to_bat = b"hi bat!".to_vec();
//     ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();
//
//     // Ensure bat receives the message from ant.
//     let Ok(msg) = bat_from_gossip_rx.recv().await else {
//         panic!("expected msg from ant")
//     };
//
//     assert_eq!(msg, ant_msg_to_bat);
//
//     // Send message from bat to ant.
//     let bat_msg_to_ant = b"oh hey ant!".to_vec();
//     bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();
//
//     // Ensure ant receives the message from bat.
//     let Ok(msg) = ant_from_gossip_rx.recv().await else {
//         panic!("expected msg from bat")
//     };
//
//     assert_eq!(msg, bat_msg_to_ant);
//
//     // Ant is going offline (stop actors and router).
//     ant_gossip_actor.stop(None);
//     ant_gossip_actor_handle.await.unwrap();
//     ant_address_book_actor.stop(None);
//     ant_address_book_actor_handle.await.unwrap();
//     ant_router.shutdown().await.unwrap();
//
//     // Cat joins the gossip topic (using ant as bootstrap).
//     let cat_peers = vec![ant_public_key];
//
//     let (cat_to_gossip, cat_from_gossip) =
//         call!(cat_gossip_actor, ToGossip::Subscribe, topic, cat_peers).unwrap();
//
//     let mut cat_from_gossip_rx = cat_from_gossip.subscribe();
//
//     // Send message from cat to bat.
//     let cat_msg_to_bat = b"hi bat!".to_vec();
//     cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();
//
//     // Briefly sleep to allow processing of sent message.
//     sleep(Duration::from_millis(50)).await;
//
//     // Ensure bat has not received the message from cat.
//     assert_eq!(bat_from_gossip_rx.try_recv(), Err(TryRecvError::Empty));
//
//     // Send message from bat to cat.
//     let bat_msg_to_cat = b"anyone out there?".to_vec();
//     bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();
//
//     // Briefly sleep to allow processing of sent message.
//     sleep(Duration::from_millis(50)).await;
//
//     // Ensure cat has not received the message from bat.
//     assert_eq!(cat_from_gossip_rx.try_recv(), Err(TryRecvError::Empty));
//
//     // At this point we have proof of partition; bat and cat are subscribed to the same gossip
//     // topic but cannot "hear" one another.
//
//     // Obtain bat's endpoint information including direct addresses.
//     let bat_endpoint_info: EndpointInfo = bat_endpoint.addr().into();
//
//     // Cat discovers bat through some out-of-band process.
//     cat_discovery.add_endpoint_info(bat_endpoint_info);
//
//     // Cat explicitly joins bat on the gossip topic.
//     let _ = cat_gossip_actor.cast(ToGossip::JoinPeers(topic, vec![bat_public_key]));
//
//     // Send message from cat to bat.
//     let cat_msg_to_bat = b"you there bat?".to_vec();
//     cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();
//
//     // Briefly sleep to allow processing of sent message.
//     sleep(Duration::from_millis(50)).await;
//
//     // Ensure bat receives the message from cat.
//     let Ok(msg) = bat_from_gossip_rx.recv().await else {
//         panic!("expected msg from cat")
//     };
//
//     assert_eq!(msg, cat_msg_to_bat);
//
//     // Send message from bat to cat.
//     let bat_msg_to_cat = b"yoyo!".to_vec();
//     bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();
//
//     // Briefly sleep to allow processing of sent message.
//     sleep(Duration::from_millis(500)).await;
//
//     // Ensure cat receives the message from bat.
//     let Ok(msg) = cat_from_gossip_rx.recv().await else {
//         panic!("expected msg from bat")
//     };
//
//     assert_eq!(msg, bat_msg_to_cat);
//
//     // Stop gossip actors.
//     bat_gossip_actor.stop(None);
//     cat_gossip_actor.stop(None);
//     bat_gossip_actor_handle.await.unwrap();
//     cat_gossip_actor_handle.await.unwrap();
//
//     // Stop address book actors.
//     bat_address_book_actor.stop(None);
//     cat_address_book_actor.stop(None);
//     bat_address_book_actor_handle.await.unwrap();
//     cat_address_book_actor_handle.await.unwrap();
//
//     // Shutdown routers.
//     bat_router.shutdown().await.unwrap();
//     cat_router.shutdown().await.unwrap();
// }
//
// #[tokio::test]
// async fn using_endpoint_actor() {
//     setup_logging();
//
//     let (mut alice_args, alice_store, _) = test_args_from_seed([112; 32]);
//     let (mut bob_args, bob_store, _) = test_args_from_seed([113; 32]);
//
//     let alice_namespace = generate_actor_namespace(&alice_args.public_key);
//     let bob_namespace = generate_actor_namespace(&bob_args.public_key);
//
//     let topic = [99; 32];
//
//     // Generate node info for both parties.
//     let alice_info = generate_node_info(&mut alice_args);
//     let bob_info = generate_node_info(&mut bob_args);
//
//     // Alice knows about bob beforehands.
//     alice_store
//         .insert_node_info(bob_info.clone())
//         .await
//         .unwrap();
//
//     // .. and vice-versa
//     bob_store
//         .insert_node_info(alice_info.clone())
//         .await
//         .unwrap();
//
//     let thread_pool = ThreadLocalActorSpawner::new();
//
//     // Spawn address books for both.
//     let (alice_address_book_actor, alice_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &alice_namespace)),
//         (alice_args.clone(), alice_store),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (bob_address_book_actor, bob_address_book_actor_handle) = AddressBook::spawn(
//         Some(with_namespace(ADDRESS_BOOK, &bob_namespace)),
//         (bob_args.clone(), bob_store),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//
//     // Spawn both endpoint actors.
//     let (alice_endpoint_actor, alice_endpoint_actor_handle) = IrohEndpoint::spawn(
//         Some(with_namespace(IROH_ENDPOINT, &alice_namespace)),
//         alice_args.clone(),
//         thread_pool.clone(),
//     )
//     .await
//     .expect("actor spawns successfully");
//     let (bob_endpoint_actor, bob_endpoint_actor_handle) = IrohEndpoint::spawn(
//         Some(with_namespace(IROH_ENDPOINT, &bob_namespace)),
//         bob_args.clone(),
//         thread_pool.clone(),
//     )
//     .await
//     .expect("actor spawns successfully");
//
//     // Receive iroh::Endpoint object, it's required for iroh-gossip.
//     let alice_endpoint = call!(alice_endpoint_actor, ToIrohEndpoint::Endpoint).unwrap();
//     let bob_endpoint = call!(bob_endpoint_actor, ToIrohEndpoint::Endpoint).unwrap();
//
//     // Spawn gossip managers for both.
//     let (alice_gossip_actor, alice_gossip_actor_handle) = TestGossip::spawn(
//         None,
//         (alice_args.clone(), alice_endpoint),
//         thread_pool.clone(),
//     )
//     .await
//     .unwrap();
//     let (bob_gossip_actor, bob_gossip_actor_handle) =
//         TestGossip::spawn(None, (bob_args.clone(), bob_endpoint), thread_pool.clone())
//             .await
//             .unwrap();
//
//     // We need to explicitly register the protocol in our endpoints.
//     //
//     // @TODO: This is currently required since the other tests do _not_ use our endpoint actor and
//     // would fail otherwise (because they would then expect that actor to exist).
//     alice_gossip_actor
//         .send_message(ToGossip::RegisterProtocol)
//         .unwrap();
//     bob_gossip_actor
//         .send_message(ToGossip::RegisterProtocol)
//         .unwrap();
//
//     // Allow time to register protocols.
//     sleep(Duration::from_millis(500)).await;
//
//     // Both peers subscribe to the gossip overlay for the same topic.
//     let (alice_to_gossip_tx, alice_from_gossip_tx) = call!(
//         alice_gossip_actor,
//         ToGossip::Subscribe,
//         topic,
//         vec![bob_info.id()]
//     )
//     .unwrap();
//     let (_bob_to_gossip_tx, bob_from_gossip_tx) = call!(
//         bob_gossip_actor,
//         ToGossip::Subscribe,
//         topic,
//         vec![alice_info.id()]
//     )
//     .unwrap();
//
//     // Allow time for joining the gossip overlays.
//     sleep(Duration::from_millis(1000)).await;
//
//     // Subscribe to sender to obtain receiver.
//     let mut _alice_from_gossip_rx = alice_from_gossip_tx.subscribe();
//     let mut bob_from_gossip_rx = bob_from_gossip_tx.subscribe();
//
//     // Send message from ant to bat.
//     alice_to_gossip_tx.send(b"hi!".to_vec()).await.unwrap();
//
//     // Allow time to process sent message.
//     sleep(Duration::from_millis(500)).await;
//
//     // Ensure bat receives the message from ant.
//     let Ok(msg) = bob_from_gossip_rx.recv().await else {
//         panic!("expected msg from ant")
//     };
//     assert_eq!(msg, b"hi!".to_vec());
//
//     // Stop gossip actors.
//     alice_gossip_actor.stop(None);
//     alice_gossip_actor_handle.await.unwrap();
//     bob_gossip_actor.stop(None);
//     bob_gossip_actor_handle.await.unwrap();
//
//     // Stop endpoint actors.
//     alice_endpoint_actor.stop(None);
//     alice_endpoint_actor_handle.await.unwrap();
//     bob_endpoint_actor.stop(None);
//     bob_endpoint_actor_handle.await.unwrap();
//
//     // Stop address book actors.
//     alice_address_book_actor.stop(None);
//     alice_address_book_actor_handle.await.unwrap();
//     bob_address_book_actor.stop(None);
//     bob_address_book_actor_handle.await.unwrap();
// }
