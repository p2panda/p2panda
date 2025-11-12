// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use p2panda_core::PrivateKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use rand::Rng;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{DISCOVERY_MANAGER, DiscoveryManager, ToDiscoveryManager};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::args::ApplicationArguments;
use crate::args::test_utils::{test_args, test_args_from_seed};
use crate::{NodeId, NodeInfo, TopicId, TransportAddress, UnsignedTransportInfo};

use super::DiscoveryActorName;

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

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
    address_book_ref: ActorRef<ToAddressBook<TopicId>>,
    discovery_manager_ref: ActorRef<ToDiscoveryManager<TopicId>>,
    thread_pool: ThreadLocalActorSpawner,
}

impl TestNode {
    pub async fn spawn(seed: [u8; 32]) -> Self {
        let (args, store) = test_args_from_seed(seed);
        let actor_namespace = generate_actor_namespace(&args.public_key);
        let thread_pool = ThreadLocalActorSpawner::new();

        IrohEndpoint::spawn(
            Some(with_namespace(IROH_ENDPOINT, &actor_namespace)),
            args.clone(),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        let (address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        let (discovery_manager_ref, discovery_manager_handle) = DiscoveryManager::spawn(
            Some(with_namespace(DISCOVERY_MANAGER, &actor_namespace)),
            (args.clone(), store.clone()),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        Self {
            args,
            address_book_ref,
            discovery_manager_ref,
            thread_pool,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub fn node_info(&mut self) -> NodeInfo {
        let mut transport_info = UnsignedTransportInfo::from_addrs([TransportAddress::from_iroh(
            self.args.public_key,
            None,
            [(
                self.args.iroh_config.bind_ip_v4,
                self.args.iroh_config.bind_port_v4,
            )
                .into()],
        )]);
        transport_info.timestamp = self.args.rng.random::<u32>() as u64;
        let transport_info = transport_info.sign(&self.args.private_key).unwrap();

        NodeInfo {
            node_id: self.args.public_key,
            bootstrap: false,
            transports: Some(transport_info),
        }
    }
}

#[tokio::test]
async fn smoke_test() {
    setup_logging();

    let mut alice = TestNode::spawn([1; 32]).await;
    let mut bob = TestNode::spawn([2; 32]).await;

    // Alice inserts Bob's info in address book and marks it as a bootstrap node.
    call!(alice.address_book_ref, ToAddressBook::InsertNodeInfo, {
        let mut info = bob.node_info();
        info.bootstrap = true;
        info
    })
    .unwrap();

    sleep(Duration::from_secs(5)).await;

    let metrics = call!(alice.discovery_manager_ref, ToDiscoveryManager::Metrics).unwrap();
    println!("{:?}", metrics);
}
