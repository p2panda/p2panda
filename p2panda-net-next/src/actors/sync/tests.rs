// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;

use p2panda_discovery::address_book::AddressBookStore as _;
use p2panda_sync::FromSync;
use p2panda_sync::traits::{Protocol, SyncManager};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::gossip::{GOSSIP, Gossip};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, EventuallyConsistentStreams, ToEventuallyConsistentStreams,
};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::test_utils::{
    NoSyncConfig, NoSyncEvent, NoSyncManager, NoSyncMessage, generate_node_info, setup_logging,
    test_args_from_seed,
};

struct TestNode<M>
where
    M: SyncManager<TopicId> + Send + 'static,
    M::Error: StdError + Send + Sync + 'static,
    M::Protocol: Send + 'static,
    <M::Protocol as Protocol>::Event: Clone + Debug + Send + Sync + 'static,
    <M::Protocol as Protocol>::Error: StdError + Send + Sync + 'static,
{
    args: ApplicationArguments,
    address_book_ref: ActorRef<ToAddressBook>,
    stream_ref: ActorRef<ToEventuallyConsistentStreams<<M::Protocol as Protocol>::Event>>,
    #[allow(unused)]
    thread_pool: ThreadLocalActorSpawner,
}

impl<M> TestNode<M>
where
    M: SyncManager<TopicId> + Send + 'static,
    M::Error: StdError + Send + Sync + 'static,
    M::Protocol: Send + 'static,
    <M::Protocol as Protocol>::Event: Clone + Debug + Send + Sync + 'static,
    <M::Protocol as Protocol>::Error: StdError + Send + Sync + 'static,
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
        let (address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (args.clone(), store.clone()),
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
        let (gossip_actor, _) = Gossip::<<M::Protocol as Protocol>::Event>::spawn(
            Some(with_namespace(GOSSIP, &actor_namespace)),
            (args.clone(), endpoint),
            args.root_thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the eventually consistent streams actor.
        let (stream_ref, _) = EventuallyConsistentStreams::<M>::spawn(
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
            address_book_ref,
            stream_ref,
            thread_pool,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub fn shutdown(&self) {
        self.stream_ref.stop(None);
        self.address_book_ref.stop(None);
    }
}

#[tokio::test]
async fn e2e_sync() {
    setup_logging();

    let topic_id = [0; 32];

    let (bob_sync_config, _bob_rx) = NoSyncConfig::new();
    let mut bob: TestNode<NoSyncManager> = TestNode::spawn([11; 32], vec![], bob_sync_config).await;

    let (alice_sync_config, _alice_rx) = NoSyncConfig::new();
    let alice: TestNode<NoSyncManager> = TestNode::spawn(
        [10; 32],
        vec![generate_node_info(&mut bob.args)],
        alice_sync_config,
    )
    .await;

    let alice_stream = call!(
        alice.stream_ref,
        ToEventuallyConsistentStreams::Create,
        topic_id,
        false
    )
    .unwrap();
    let alice_subscription = alice_stream.subscribe().await.unwrap();

    let bob_stream = call!(
        bob.stream_ref,
        ToEventuallyConsistentStreams::Create,
        topic_id,
        false
    )
    .unwrap();
    let bob_subscription = bob_stream.subscribe().await.unwrap();

    alice
        .stream_ref
        .cast(ToEventuallyConsistentStreams::InitiateSync(
            topic_id,
            bob.node_id(),
        ))
        .unwrap();

    for (mut sub, expected_remote) in [
        (alice_subscription, bob.node_id()),
        (bob_subscription, alice.node_id()),
    ] {
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: NoSyncEvent::SessionCreated
            } if remote == expected_remote
        ));
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: NoSyncEvent::SyncStarted
            } if remote == expected_remote
        ));
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: NoSyncEvent::Received(NoSyncMessage::Data)
            } if remote == expected_remote
        ));
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: NoSyncEvent::SyncFinished
            } if remote == expected_remote
        ));
    }

    alice.shutdown();
    bob.shutdown();
}
