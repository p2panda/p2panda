// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use assert_matches::assert_matches;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_sync::traits::SyncManager;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{
    DISCOVERY_MANAGER, DiscoveryEvent, DiscoveryManager, ToDiscoveryManager,
};
use crate::actors::events::{EVENTS, Events, ToEvents};
use crate::actors::gossip::{GOSSIP, Gossip, ToGossip};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, EventuallyConsistentStreams, ToEventuallyConsistentStreams,
};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::test_utils::{
    NoSyncConfig, NoSyncManager, generate_node_info, setup_logging, test_args_from_seed,
};

use super::NetworkEvent;

struct TestNode<M>
where
    M: SyncManager<TopicId> + Debug + Send + 'static,
{
    args: ApplicationArguments,
    address_book_actor: ActorRef<ToAddressBook>,
    discovery_actor: ActorRef<ToDiscoveryManager>,
    endpoint_actor: ActorRef<ToIrohEndpoint>,
    events_actor: ActorRef<ToEvents>,
    gossip_actor: ActorRef<ToGossip>,
    stream_actor: ActorRef<ToEventuallyConsistentStreams<M>>,
    #[allow(unused)]
    thread_pool: ThreadLocalActorSpawner,
}

impl<M> TestNode<M>
where
    M: SyncManager<TopicId> + Debug + Send + 'static,
{
    pub async fn spawn(seed: [u8; 32], node_infos: Vec<NodeInfo>, sync_config: M::Config) -> Self {
        let (args, store, _) = test_args_from_seed(seed);
        let actor_namespace = generate_actor_namespace(&args.public_key);
        let thread_pool = ThreadLocalActorSpawner::new();

        // Pre-populate the address book with known addresses.
        for info in node_infos {
            store.insert_node_info(info).await.unwrap();
        }

        // Spawn the address book actor.
        let (address_book_actor, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (args.clone(), store.clone()),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the events actor.
        let (events_actor, _) = Events::spawn(
            Some(with_namespace(EVENTS, &actor_namespace)),
            args.clone(),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the endpoint actor.
        let (endpoint_actor, _) = IrohEndpoint::spawn(
            Some(with_namespace(IROH_ENDPOINT, &actor_namespace)),
            args.clone(),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        let endpoint = call!(endpoint_actor, ToIrohEndpoint::Endpoint).unwrap();

        // Spawn the gossip actor.
        let (gossip_actor, _) = Gossip::<M>::spawn(
            Some(with_namespace(GOSSIP, &actor_namespace)),
            (args.clone(), endpoint),
            args.root_thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the discovery mananger actor.
        let (discovery_actor, _) = DiscoveryManager::spawn(
            Some(with_namespace(DISCOVERY_MANAGER, &actor_namespace)),
            (args.clone(), store.clone()),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the eventually consistent streams actor.
        let (stream_actor, _) = EventuallyConsistentStreams::<M>::spawn(
            Some(with_namespace(
                EVENTUALLY_CONSISTENT_STREAMS,
                &actor_namespace,
            )),
            (args.clone(), gossip_actor.clone(), sync_config.clone()),
            args.root_thread_pool.clone(),
        )
        .await
        .unwrap();

        Self {
            args,
            address_book_actor,
            discovery_actor,
            endpoint_actor,
            events_actor,
            gossip_actor,
            stream_actor,
            thread_pool,
        }
    }

    pub fn _node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub async fn shutdown(self) {
        self.discovery_actor.stop(None);
        self.events_actor.stop(None);
        self.stream_actor.stop(None);
        self.gossip_actor.stop(None);
        self.endpoint_actor.stop(None);
        self.address_book_actor.stop(None);
    }
}

#[tokio::test]
async fn discovery_events_are_received() {
    setup_logging();

    let (bob_sync_config, _bob_rx) = NoSyncConfig::new();
    let mut bob: TestNode<NoSyncManager> = TestNode::spawn([11; 32], vec![], bob_sync_config).await;

    let (alice_sync_config, _alice_rx) = NoSyncConfig::new();
    let alice: TestNode<NoSyncManager> = TestNode::spawn(
        [10; 32],
        vec![generate_node_info(&mut bob.args)],
        alice_sync_config,
    )
    .await;

    let mut alice_events = call!(alice.events_actor, ToEvents::Subscribe).unwrap();

    let alice_event = alice_events.recv().await.unwrap();
    assert_matches!(
        alice_event,
        NetworkEvent::Discovery(DiscoveryEvent::SessionStarted { .. })
    );

    let alice_event = alice_events.recv().await.unwrap();
    assert_matches!(
        alice_event,
        NetworkEvent::Discovery(DiscoveryEvent::SessionEnded { .. })
    );

    alice.shutdown().await;
    bob.shutdown().await;
}
