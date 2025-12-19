// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::AddressBookStore as _;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use tokio::task::JoinHandle;

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{
    DISCOVERY_MANAGER, DiscoveryEvent, DiscoveryManager, SessionRole, ToDiscoveryManager,
};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::test_utils::{generate_trusted_node_info, setup_logging, test_args_from_seed};

use super::DiscoveryActorName;

#[test]
fn actor_name_helper() {
    let public_key = PrivateKey::new().public_key();
    let actor_namespace = &generate_actor_namespace(&public_key);
    let value = DiscoveryActorName::new_walker(6).to_string(actor_namespace);
    assert_eq!(
        DiscoveryActorName::from_string(&value),
        DiscoveryActorName::Walker { walker_id: 6 }
    );
}

struct TestNode {
    args: ApplicationArguments,
    #[allow(unused)]
    endpoint_ref: ActorRef<ToIrohEndpoint>,
    address_book_ref: ActorRef<ToAddressBook>,
    discovery_manager_ref: ActorRef<ToDiscoveryManager>,
    #[allow(unused)]
    thread_pool: ThreadLocalActorSpawner,
}

impl TestNode {
    pub async fn spawn(seed: [u8; 32], node_infos: Vec<NodeInfo>) -> Self {
        let (args, store, _) = test_args_from_seed(seed);

        // Pre-populate the address book with known addresses.
        for info in node_infos {
            store.insert_node_info(info).await.unwrap();
        }

        let actor_namespace = generate_actor_namespace(&args.public_key);
        let thread_pool = ThreadLocalActorSpawner::new();

        let (address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (args.clone(), store.clone()),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        let (endpoint_ref, _) = IrohEndpoint::spawn(
            Some(with_namespace(IROH_ENDPOINT, &actor_namespace)),
            args.clone(),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        let (discovery_manager_ref, _) = DiscoveryManager::spawn(
            Some(with_namespace(DISCOVERY_MANAGER, &actor_namespace)),
            (args.clone(), store.clone()),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        Self {
            args,
            endpoint_ref,
            address_book_ref,
            discovery_manager_ref,
            thread_pool,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub fn shutdown(&self) {
        self.address_book_ref.stop(None);
        self.discovery_manager_ref.stop(None);
        self.endpoint_ref.stop(None);
    }
}

async fn session_ended_handle(actor_ref: &ActorRef<ToDiscoveryManager>) -> JoinHandle<()> {
    let mut events = call!(actor_ref, ToDiscoveryManager::Events).unwrap();

    tokio::spawn(async move {
        loop {
            let event = events.recv().await.unwrap();
            if let DiscoveryEvent::SessionEnded {
                role: SessionRole::Initiated,
                ..
            } = event
            {
                break;
            }
        }
    })
}

#[tokio::test]
async fn smoke_test() {
    setup_logging();

    // Bob's address book is empty;
    let mut bob = TestNode::spawn([17; 32], vec![]).await;

    // Alice inserts Bob's info in their address book.
    let alice = TestNode::spawn([18; 32], vec![generate_trusted_node_info(&mut bob.args)]).await;

    // Wait until both parties finished at least one discovery session.
    let alice_session_ended = session_ended_handle(&alice.discovery_manager_ref).await;
    let bob_session_ended = session_ended_handle(&bob.discovery_manager_ref).await;
    alice_session_ended.await.unwrap();
    bob_session_ended.await.unwrap();

    // Alice didn't learn about new transport info of Bob as their manually added node info was
    // already the "latest".
    let alice_metrics = call!(alice.discovery_manager_ref, ToDiscoveryManager::Metrics).unwrap();
    assert_eq!(alice_metrics.newly_learned_transport_infos, 0);

    // Bob learned of Alice.
    let bob_metrics = call!(bob.discovery_manager_ref, ToDiscoveryManager::Metrics).unwrap();
    assert_eq!(bob_metrics.newly_learned_transport_infos, 1);

    // Alice should now be in the address book of Bob.
    let result = call!(
        bob.address_book_ref,
        ToAddressBook::NodeInfo,
        alice.node_id()
    )
    .unwrap();
    assert!(result.is_some());

    alice.shutdown();
    bob.shutdown();
}
