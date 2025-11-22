// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::AddressBookStore as _;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use tokio::time::sleep;

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{DISCOVERY_MANAGER, DiscoveryManager, ToDiscoveryManager};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::test_utils::{generate_node_info, setup_logging, test_args_from_seed};

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
    }
}

#[tokio::test]
async fn smoke_test() {
    setup_logging();

    // Bob's address book is empty;
    let mut bob = TestNode::spawn([17; 32], vec![]).await;

    // Alice inserts Bob's info in address book.
    let alice = TestNode::spawn([18; 32], vec![generate_node_info(&mut bob.args)]).await;

    sleep(Duration::from_millis(100)).await;

    // Alice didn't learn about new transport info of Bob.
    let alice_metrics = call!(alice.discovery_manager_ref, ToDiscoveryManager::Metrics).unwrap();
    assert_eq!(alice_metrics.newly_learned_transport_infos, 1);

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
