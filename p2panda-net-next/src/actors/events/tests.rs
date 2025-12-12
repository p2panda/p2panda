// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_discovery::address_book::AddressBookStore;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{
    DISCOVERY_MANAGER, DiscoveryEvent, DiscoveryManager, ToDiscoveryManager,
};
use crate::actors::events::{EVENTS, Events, ToEvents};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::test_utils::{generate_trusted_node_info, setup_logging, test_args_from_seed};

use super::NetworkEvent;

struct TestNode {
    args: ApplicationArguments,
    address_book_actor: ActorRef<ToAddressBook>,
    discovery_actor: ActorRef<ToDiscoveryManager>,
    endpoint_actor: ActorRef<ToIrohEndpoint>,
    events_actor: ActorRef<ToEvents>,
    #[allow(unused)]
    thread_pool: ThreadLocalActorSpawner,
}

impl TestNode {
    pub async fn spawn(seed: [u8; 32], node_infos: Vec<NodeInfo>) -> Self {
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
            (args.clone(), None),
            thread_pool.clone(),
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

        Self {
            args,
            address_book_actor,
            discovery_actor,
            endpoint_actor,
            events_actor,
            thread_pool,
        }
    }

    pub fn _node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub async fn shutdown(self) {
        self.discovery_actor.stop(None);
        self.events_actor.stop(None);
        self.endpoint_actor.stop(None);
        self.address_book_actor.stop(None);
    }
}

#[tokio::test]
async fn discovery_events_are_received() {
    setup_logging();

    let mut bob: TestNode = TestNode::spawn([133; 32], vec![]).await;
    let alice: TestNode =
        TestNode::spawn([134; 32], vec![generate_trusted_node_info(&mut bob.args)]).await;

    let mut alice_events = call!(alice.events_actor, ToEvents::Subscribe).unwrap();

    let mut received_started = 0;
    let mut received_ended = 0;

    loop {
        match alice_events.recv().await.unwrap() {
            NetworkEvent::Discovery(DiscoveryEvent::SessionStarted { .. }) => {
                received_started += 1;
            }
            NetworkEvent::Discovery(DiscoveryEvent::SessionEnded { .. }) => {
                received_ended += 1;
            }
            _ => (),
        }

        // We've received at least one started and ended event.
        if received_started > 0 && received_ended > 0 {
            break;
        }
    }

    alice.shutdown().await;
    bob.shutdown().await;
}
