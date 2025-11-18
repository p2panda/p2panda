// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use iroh::discovery::EndpointInfo;
use iroh::discovery::static_provider::StaticProvider;
use iroh::protocol::Router as IrohRouter;
use iroh::{Endpoint as IrohEndpoint, RelayMode};
use p2panda_core::PublicKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::time::sleep;

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, AddressBook};
use crate::actors::gossip::session::ToGossipSession;
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::protocols::hash_protocol_id_with_network_id;
use crate::test_utils::test_args;
use crate::utils::from_private_key;

use super::{Gossip, GossipState, ToGossip};

type TestGossip = Gossip<()>;

// Use this internal type to introspect the actor's current state.
pub struct DebugState {
    neighbours: HashMap<TopicId, HashSet<PublicKey>>,
    sessions_by_topic: HashMap<TopicId, ActorRef<ToGossipSession>>,
}

impl From<&mut GossipState> for DebugState {
    fn from(value: &mut GossipState) -> Self {
        Self {
            neighbours: value.neighbours.clone(),
            sessions_by_topic: value.sessions.sessions_by_topic.clone(),
        }
    }
}

#[tokio::test]
async fn correct_termination_state() {
    // This test asserts that the state of `sessions_by_topic` and `neighbours_by_topic`
    // is correctly updated within the `Gossip` actor.
    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap peer
    // - Cat joins the gossip topic using ant as bootstrap peer
    // - Terminate ant's gossip actor
    // - Assert: Ant's gossip actor state includes the topic that was subscribed to
    // - Assert: Ant's gossip actor state maps the subscribed topic to the public keys of
    //           bat and cat (neighbours)

    let (ant_args, ant_store, _) = test_args();
    let (bat_args, bat_store, _) = test_args();
    let (cat_args, cat_store, _) = test_args();

    let mixed_alpn = hash_protocol_id_with_network_id(&iroh_gossip::ALPN, &ant_args.network_id);

    // Create topic.
    let topic = [3; 32];

    // Create keypairs.
    let ant_private_key = ant_args.private_key.clone();
    let bat_private_key = bat_args.private_key.clone();
    let cat_private_key = cat_args.private_key.clone();

    let ant_public_key = ant_private_key.public_key();
    let bat_public_key = bat_private_key.public_key();
    let cat_public_key = cat_private_key.public_key();

    // Create endpoints.
    let ant_discovery = StaticProvider::new();
    let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(ant_private_key))
        .discovery(ant_discovery.clone())
        .bind()
        .await
        .unwrap();

    let bat_discovery = StaticProvider::new();
    let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(bat_private_key))
        .discovery(bat_discovery.clone())
        .bind()
        .await
        .unwrap();

    let cat_discovery = StaticProvider::new();
    let cat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(cat_private_key))
        .discovery(cat_discovery.clone())
        .bind()
        .await
        .unwrap();

    // Obtain ant's endpoint information including direct addresses.
    let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

    // Bat discovers ant through some out-of-band process.
    bat_discovery.add_endpoint_info(ant_endpoint_info.clone());

    // Cat discovers ant through some out-of-band process.
    cat_discovery.add_endpoint_info(ant_endpoint_info);

    let thread_pool = ThreadLocalActorSpawner::new();

    // Spawn one address book for each peer.
    let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
    let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
    let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);

    let (ant_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
        (ant_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (bat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
        (bat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (cat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
        (cat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();

    // Spawn gossip actors.
    let (ant_gossip_actor, ant_gossip_actor_handle) =
        TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (bat_gossip_actor, bat_gossip_actor_handle) =
        TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (cat_gossip_actor, cat_gossip_actor_handle) =
        TestGossip::spawn(None, (cat_args, cat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();

    // Get handles to gossip.
    let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
    let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
    let cat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();

    // Build and spawn routers.
    let ant_router = IrohRouter::builder(ant_endpoint.clone())
        .accept(&mixed_alpn, ant_gossip)
        .spawn();
    let bat_router = IrohRouter::builder(bat_endpoint.clone())
        .accept(&mixed_alpn, bat_gossip)
        .spawn();
    let cat_router = IrohRouter::builder(cat_endpoint.clone())
        .accept(&mixed_alpn, cat_gossip)
        .spawn();

    // Subscribe to the gossip topic.
    let ant_peers = Vec::new();
    let bat_peers = vec![ant_public_key];
    let cat_peers = vec![ant_public_key];

    let (_ant_to_gossip, _ant_from_gossip) =
        call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
    let (_bat_to_gossip, mut _bat_from_gossip) =
        call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();
    let (_cat_to_gossip, mut _cat_from_gossip) =
        call!(cat_gossip_actor, ToGossip::Subscribe, topic, cat_peers).unwrap();

    // Briefly sleep to allow overlay to form.
    sleep(Duration::from_millis(100)).await;

    // Ensure state expectations are correct for ant's gossip actor.
    let ant_state = call!(ant_gossip_actor, ToGossip::DebugState).unwrap();
    assert!(ant_state.sessions_by_topic.contains_key(&topic));
    let neighbours = ant_state.neighbours.get(&topic).unwrap();
    assert!(neighbours.contains(&bat_public_key));
    assert!(neighbours.contains(&cat_public_key));

    // Stop all other actors and routers.
    ant_gossip_actor.stop(None);
    bat_gossip_actor.stop(None);
    cat_gossip_actor.stop(None);
    ant_gossip_actor_handle.await.unwrap();
    bat_gossip_actor_handle.await.unwrap();
    cat_gossip_actor_handle.await.unwrap();

    // Stop address book actors.
    ant_address_book_ref.stop(None);
    bat_address_book_ref.stop(None);
    cat_address_book_ref.stop(None);

    ant_router.shutdown().await.unwrap();
    bat_router.shutdown().await.unwrap();
    cat_router.shutdown().await.unwrap();
}

#[tokio::test]
async fn two_peer_gossip() {
    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap peer
    // - Assert: Ant and bat can exchange messages

    let (ant_args, ant_store, _) = test_args();
    let (bat_args, bat_store, _) = test_args();

    let mixed_alpn = hash_protocol_id_with_network_id(&iroh_gossip::ALPN, &ant_args.network_id);

    let topic = [7; 32];

    // Create keypairs.
    let ant_private_key = ant_args.private_key.clone();
    let bat_private_key = bat_args.private_key.clone();

    let ant_public_key = ant_private_key.public_key();

    // Create endpoints.
    let ant_discovery = StaticProvider::new();
    let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(ant_private_key))
        .discovery(ant_discovery.clone())
        .bind()
        .await
        .unwrap();

    let bat_discovery = StaticProvider::new();
    let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(bat_private_key))
        .discovery(bat_discovery.clone())
        .bind()
        .await
        .unwrap();

    // Obtain ant's endpoint information including direct addresses.
    let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

    // Bat discovers ant through some out-of-band process.
    bat_discovery.add_endpoint_info(ant_endpoint_info);

    let thread_pool = ThreadLocalActorSpawner::new();

    // Spawn one address book for each peer.
    let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
    let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);

    let (ant_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
        (ant_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (bat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
        (bat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();

    // Spawn gossip actors.
    let (ant_gossip_actor, ant_gossip_actor_handle) =
        TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (bat_gossip_actor, bat_gossip_actor_handle) =
        TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();

    // Get handles to gossip.
    let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
    let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();

    // Build and spawn routers.
    let ant_router = IrohRouter::builder(ant_endpoint.clone())
        .accept(&mixed_alpn, ant_gossip)
        .spawn();
    let bat_router = IrohRouter::builder(bat_endpoint.clone())
        .accept(&mixed_alpn, bat_gossip)
        .spawn();

    // Subscribe to the gossip topic.
    let ant_peers = Vec::new();
    let bat_peers = vec![ant_public_key];

    let (ant_to_gossip, ant_from_gossip) =
        call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
    let (bat_to_gossip, bat_from_gossip) =
        call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();

    // Briefly sleep to allow overlay to form.
    sleep(Duration::from_millis(100)).await;

    // Subscribe to sender to obtain receiver.
    let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
    let mut ant_from_gossip_rx = ant_from_gossip.subscribe();

    // Send message from ant to bat.
    let ant_msg_to_bat = b"hi bat!".to_vec();
    ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();

    // Ensure bat receives the message from ant.
    let Ok(msg) = bat_from_gossip_rx.recv().await else {
        panic!("expected msg from ant")
    };

    assert_eq!(msg, ant_msg_to_bat);

    // Send message from bat to ant.
    let bat_msg_to_ant = b"oh hey ant!".to_vec();
    bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();

    // Ensure ant receives the message from bat.
    let Ok(msg) = ant_from_gossip_rx.recv().await else {
        panic!("expected msg from bat")
    };

    assert_eq!(msg, bat_msg_to_ant);

    // Stop gossip actors.
    ant_gossip_actor.stop(None);
    bat_gossip_actor.stop(None);
    ant_gossip_actor_handle.await.unwrap();
    bat_gossip_actor_handle.await.unwrap();

    // Stop address book actors.
    ant_address_book_ref.stop(None);
    bat_address_book_ref.stop(None);

    // Shutdown routers.
    bat_router.shutdown().await.unwrap();
    ant_router.shutdown().await.unwrap();
}

// @TODO: This test keeps hanging at random times.
#[ignore]
#[tokio::test]
async fn third_peer_joins_non_bootstrap() {
    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap peer
    // - Cat joins the gossip topic using bat as bootstrap peer
    // - Assert: Ant, bat and cat can exchange messages

    let (ant_args, ant_store, _) = test_args();
    let (bat_args, bat_store, _) = test_args();
    let (cat_args, cat_store, _) = test_args();

    let mixed_alpn = hash_protocol_id_with_network_id(&iroh_gossip::ALPN, &ant_args.network_id);

    let topic = [11; 32];

    // Create keypairs.
    let ant_private_key = ant_args.private_key.clone();
    let bat_private_key = bat_args.private_key.clone();
    let cat_private_key = cat_args.private_key.clone();

    let ant_public_key = ant_private_key.public_key();
    let bat_public_key = bat_private_key.public_key();

    // Create endpoints.
    let ant_discovery = StaticProvider::new();
    let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(ant_private_key))
        .discovery(ant_discovery.clone())
        .bind()
        .await
        .unwrap();

    let bat_discovery = StaticProvider::new();
    let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(bat_private_key))
        .discovery(bat_discovery.clone())
        .bind()
        .await
        .unwrap();

    let cat_discovery = StaticProvider::new();
    let cat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(cat_private_key))
        .discovery(cat_discovery.clone())
        .bind()
        .await
        .unwrap();

    // Obtain ant's endpoint information including direct addresses.
    let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

    // Bat discovers ant through some out-of-band process.
    bat_discovery.add_endpoint_info(ant_endpoint_info);

    let thread_pool = ThreadLocalActorSpawner::new();

    let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
    let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
    let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);

    let (ant_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
        (ant_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (bat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
        (bat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (cat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
        (cat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();

    // Spawn gossip actors.
    let (ant_gossip_actor, ant_gossip_actor_handle) =
        TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (bat_gossip_actor, bat_gossip_actor_handle) =
        TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (cat_gossip_actor, cat_gossip_actor_handle) =
        TestGossip::spawn(None, (cat_args, cat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();

    // Get handles to gossip.
    let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
    let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
    let cat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();

    // Build and spawn routers.
    let ant_router = IrohRouter::builder(ant_endpoint.clone())
        .accept(&mixed_alpn, ant_gossip)
        .spawn();
    let bat_router = IrohRouter::builder(bat_endpoint.clone())
        .accept(&mixed_alpn, bat_gossip)
        .spawn();
    let cat_router = IrohRouter::builder(cat_endpoint.clone())
        .accept(&mixed_alpn, cat_gossip)
        .spawn();

    // Subscribe to the gossip topic.
    let ant_peers = Vec::new();
    let bat_peers = vec![ant_public_key];

    let (ant_to_gossip, _ant_from_gossip) =
        call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
    let (_bat_to_gossip, bat_from_gossip) =
        call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();

    // Briefly sleep to allow overlay to form.
    sleep(Duration::from_millis(250)).await;

    // Subscribe to sender to obtain receiver.
    let mut bat_from_gossip_rx = bat_from_gossip.subscribe();

    // Obtain bat's endpoint information including direct addresses.
    let bat_endpoint_info: EndpointInfo = bat_endpoint.addr().into();

    // Cat discovers bat through some out-of-band process.
    cat_discovery.add_endpoint_info(bat_endpoint_info);

    let cat_peers = vec![bat_public_key];

    // Cat subscribes to topic using bat as bootstrap.
    let (cat_to_gossip, cat_from_gossip) =
        call!(cat_gossip_actor, ToGossip::Subscribe, topic, cat_peers).unwrap();

    // Briefly sleep to allow overlay to form.
    sleep(Duration::from_millis(250)).await;

    let mut cat_from_gossip_rx = cat_from_gossip.subscribe();

    // Send message from cat to ant and bat.
    let cat_msg_to_ant_and_bat = b"hi ant and bat!".to_vec();
    cat_to_gossip
        .send(cat_msg_to_ant_and_bat.clone())
        .await
        .unwrap();

    // Ensure bat receives cat's message.
    let Ok(msg) = bat_from_gossip_rx.recv().await else {
        panic!("expected msg from cat")
    };

    assert_eq!(msg, cat_msg_to_ant_and_bat);

    // Send message from ant to bat and cat.
    let ant_msg_to_bat_and_cat = b"hi bat and cat!".to_vec();
    ant_to_gossip
        .send(ant_msg_to_bat_and_cat.clone())
        .await
        .unwrap();

    // Ensure cat receives ant's message.
    let Ok(msg) = cat_from_gossip_rx.recv().await else {
        panic!("expected msg from ant")
    };

    // NOTE: In this case the message is delivered by bat; not directly from ant.
    assert_eq!(msg, ant_msg_to_bat_and_cat);

    // Stop gossip actors.
    ant_gossip_actor.stop(None);
    bat_gossip_actor.stop(None);
    cat_gossip_actor.stop(None);
    ant_gossip_actor_handle.await.unwrap();
    bat_gossip_actor_handle.await.unwrap();
    cat_gossip_actor_handle.await.unwrap();

    // Stop address book actors.
    ant_address_book_ref.stop(None);
    bat_address_book_ref.stop(None);
    cat_address_book_ref.stop(None);

    // Shutdown routers.
    ant_router.shutdown().await.unwrap();
    bat_router.shutdown().await.unwrap();
    cat_router.shutdown().await.unwrap();
}

#[tokio::test]
async fn three_peer_gossip_with_rejoin() {
    // Scenario:
    //
    // - Ant joins the gossip topic
    // - Bat joins the gossip topic using ant as bootstrap peer
    // - Assert: Ant and bat can exchange messages
    // - Ant goes offline
    // - Cat joins the gossip topic using ant as bootstrap peer
    // - Assert: Bat and cat can't exchange messages (proof of partition)
    // - Cat learns about bat through out-of-band discovery process
    // - Cat joins bat on established gossip topic
    // - Assert: Bat and cat can now exchange messages (proof of healed partition)

    let (ant_args, ant_store, _) = test_args();
    let (bat_args, bat_store, _) = test_args();
    let (cat_args, cat_store, _) = test_args();

    let mixed_alpn = hash_protocol_id_with_network_id(&iroh_gossip::ALPN, &ant_args.network_id);

    let topic = [9; 32];

    // Create keypairs.
    let ant_private_key = ant_args.private_key.clone();
    let bat_private_key = bat_args.private_key.clone();
    let cat_private_key = cat_args.private_key.clone();

    let ant_public_key = ant_private_key.public_key();
    let bat_public_key = bat_private_key.public_key();

    // Create endpoints.
    let ant_discovery = StaticProvider::new();
    let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(ant_private_key))
        .discovery(ant_discovery.clone())
        .bind()
        .await
        .unwrap();

    let bat_discovery = StaticProvider::new();
    let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(bat_private_key))
        .discovery(bat_discovery.clone())
        .bind()
        .await
        .unwrap();

    let cat_discovery = StaticProvider::new();
    let cat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
        .secret_key(from_private_key(cat_private_key))
        .discovery(cat_discovery.clone())
        .bind()
        .await
        .unwrap();

    // Obtain ant's endpoint information including direct addresses.
    let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

    // Bat discovers ant through some out-of-band process.
    bat_discovery.add_endpoint_info(ant_endpoint_info);

    let thread_pool = ThreadLocalActorSpawner::new();

    // Spawn one address book for each peer.
    let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
    let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
    let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);

    let (ant_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
        (ant_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (bat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
        (bat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();
    let (cat_address_book_ref, _) = AddressBook::spawn(
        Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
        (cat_store.clone(),),
        thread_pool.clone(),
    )
    .await
    .unwrap();

    // Spawn gossip actors.
    let (ant_gossip_actor, ant_gossip_actor_handle) =
        TestGossip::spawn(None, (ant_args, ant_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (bat_gossip_actor, bat_gossip_actor_handle) =
        TestGossip::spawn(None, (bat_args, bat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();
    let (cat_gossip_actor, cat_gossip_actor_handle) =
        TestGossip::spawn(None, (cat_args, cat_endpoint.clone()), thread_pool.clone())
            .await
            .unwrap();

    // Get handles to gossip.
    let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
    let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
    let cat_gossip = call!(cat_gossip_actor, ToGossip::Handle).unwrap();

    // Build and spawn routers.
    let ant_router = IrohRouter::builder(ant_endpoint.clone())
        .accept(&mixed_alpn, ant_gossip)
        .spawn();
    let bat_router = IrohRouter::builder(bat_endpoint.clone())
        .accept(&mixed_alpn, bat_gossip)
        .spawn();
    let cat_router = IrohRouter::builder(cat_endpoint.clone())
        .accept(&mixed_alpn, cat_gossip)
        .spawn();

    // Ant and bat subscribe to the gossip topic.
    let ant_peers = Vec::new();
    let bat_peers = vec![ant_public_key];

    let (ant_to_gossip, ant_from_gossip) =
        call!(ant_gossip_actor, ToGossip::Subscribe, topic, ant_peers).unwrap();
    let (bat_to_gossip, bat_from_gossip) =
        call!(bat_gossip_actor, ToGossip::Subscribe, topic, bat_peers).unwrap();

    // Subscribe to sender to obtain receiver.
    let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
    let mut ant_from_gossip_rx = ant_from_gossip.subscribe();

    // Send message from ant to bat.
    let ant_msg_to_bat = b"hi bat!".to_vec();
    ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();

    // Ensure bat receives the message from ant.
    let Ok(msg) = bat_from_gossip_rx.recv().await else {
        panic!("expected msg from ant")
    };

    assert_eq!(msg, ant_msg_to_bat);

    // Send message from bat to ant.
    let bat_msg_to_ant = b"oh hey ant!".to_vec();
    bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();

    // Ensure ant receives the message from bat.
    let Ok(msg) = ant_from_gossip_rx.recv().await else {
        panic!("expected msg from bat")
    };

    assert_eq!(msg, bat_msg_to_ant);

    // Stop the gossip actor and router for ant (going offline).
    ant_gossip_actor.stop(None);
    ant_gossip_actor_handle.await.unwrap();
    ant_router.shutdown().await.unwrap();

    // Cat joins the gossip topic (using ant as bootstrap).
    let cat_peers = vec![ant_public_key];

    let (cat_to_gossip, cat_from_gossip) =
        call!(cat_gossip_actor, ToGossip::Subscribe, topic, cat_peers).unwrap();

    let mut cat_from_gossip_rx = cat_from_gossip.subscribe();

    // Send message from cat to bat.
    let cat_msg_to_bat = b"hi bat!".to_vec();
    cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(50)).await;

    // Ensure bat has not received the message from cat.
    assert_eq!(bat_from_gossip_rx.try_recv(), Err(TryRecvError::Empty));

    // Send message from bat to cat.
    let bat_msg_to_cat = b"anyone out there?".to_vec();
    bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(50)).await;

    // Ensure cat has not received the message from bat.
    assert_eq!(cat_from_gossip_rx.try_recv(), Err(TryRecvError::Empty));

    // At this point we have proof of partition; bat and cat are subscribed to the same gossip
    // topic but cannot "hear" one another.

    // Obtain bat's endpoint information including direct addresses.
    let bat_endpoint_info: EndpointInfo = bat_endpoint.addr().into();

    // Cat discovers bat through some out-of-band process.
    cat_discovery.add_endpoint_info(bat_endpoint_info);

    // Cat explicitly joins bat on the gossip topic.
    let _ = cat_gossip_actor.cast(ToGossip::JoinPeers(topic, vec![bat_public_key]));

    // Send message from cat to bat.
    let cat_msg_to_bat = b"you there bat?".to_vec();
    cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(50)).await;

    // Ensure bat receives the message from cat.
    let Ok(msg) = bat_from_gossip_rx.recv().await else {
        panic!("expected msg from cat")
    };

    assert_eq!(msg, cat_msg_to_bat);

    // Send message from bat to cat.
    let bat_msg_to_cat = b"yoyo!".to_vec();
    bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();

    // Briefly sleep to allow processing of sent message.
    sleep(Duration::from_millis(500)).await;

    // Ensure cat receives the message from bat.
    let Ok(msg) = cat_from_gossip_rx.recv().await else {
        panic!("expected msg from bat")
    };

    assert_eq!(msg, bat_msg_to_cat);

    // Stop gossip actors.
    bat_gossip_actor.stop(None);
    bat_gossip_actor_handle.await.unwrap();
    cat_gossip_actor.stop(None);
    cat_gossip_actor_handle.await.unwrap();

    // Stop address book actors.
    ant_address_book_ref.stop(None);
    bat_address_book_ref.stop(None);
    cat_address_book_ref.stop(None);

    // Shutdown routers.
    bat_router.shutdown().await.unwrap();
    cat_router.shutdown().await.unwrap();
}
