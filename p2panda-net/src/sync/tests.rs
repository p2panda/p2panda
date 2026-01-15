// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Duration;

use futures_channel::mpsc::{self, SendError};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use p2panda_sync::traits::{Protocol, SyncManager as SyncManagerTrait};
use p2panda_sync::{FromSync, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use rand::random;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::address_book::AddressBook;
use crate::addrs::NodeInfo;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::{SyncManager, ToSyncManager};
use crate::sync::handle::SyncHandle;
use crate::test_utils::{ApplicationArguments, setup_logging, test_args_from_seed};
use crate::{NodeId, TopicId};

const TEST_PROTOCOL_ID: [u8; 32] = [101; 32];

struct FailingNode {
    args: ApplicationArguments,
    sync_ref: ActorRef<ToSyncManager<DummySyncManager<FailingSyncConfig, FailingSyncProtocol>>>,
}

impl FailingNode {
    pub async fn spawn(
        seed: [u8; 32],
        node_infos: Vec<NodeInfo>,
        sync_config: FailingSyncConfig,
    ) -> Self {
        let (args, address_book_store) = test_args_from_seed(seed);

        let address_book = AddressBook::builder()
            .store(address_book_store)
            .spawn()
            .await
            .unwrap();

        // Pre-populate the address book with known addresses.
        for info in node_infos {
            address_book.insert_node_info(info).await.unwrap();
        }

        let endpoint = Endpoint::builder(address_book.clone())
            .config(args.iroh_config.clone())
            .private_key(args.private_key.clone())
            .spawn()
            .await
            .unwrap();

        let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
            .spawn()
            .await
            .unwrap();

        let thread_pool = ThreadLocalActorSpawner::new();
        let (sync_ref, _) =
            SyncManager::<DummySyncManager<FailingSyncConfig, FailingSyncProtocol>>::spawn(
                None,
                (TEST_PROTOCOL_ID.to_vec(), sync_config, endpoint, gossip),
                thread_pool,
            )
            .await
            .unwrap();

        Self { args, sync_ref }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub fn shutdown(&self) {
        self.sync_ref.stop(None);
    }
}

#[derive(Debug, Error)]
enum SyncError {
    #[error("unexpected sync failure")]
    UnexpectedFailure,
}

#[derive(Debug, Clone)]
enum SyncBehaviour {
    Panic,
    Error,
    Wait,
}

#[derive(Debug)]
struct FailingSyncProtocol {
    behaviour: SyncBehaviour,
}

impl Protocol for FailingSyncProtocol {
    type Output = ();
    type Error = SyncError;
    type Message = ();

    async fn run(
        self,
        sink: &mut (impl Sink<Self::Message, Error = impl std::fmt::Debug> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, impl std::fmt::Debug>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        // Send one message otherwise the accepting peer will not be able to accept the connection.
        let _ = sink.send(()).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        match self.behaviour {
            SyncBehaviour::Panic => panic!(),
            SyncBehaviour::Error => return Err(SyncError::UnexpectedFailure),
            SyncBehaviour::Wait => {
                while let Some(_) = stream.next().await {}
                return Err(SyncError::UnexpectedFailure);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct FailingSyncConfig {
    pub event_tx: broadcast::Sender<FromSync<DummySyncEvent>>,
    pub behaviour: SyncBehaviour,
}

impl FailingSyncConfig {
    pub fn new(behaviour: SyncBehaviour) -> (Self, broadcast::Receiver<FromSync<DummySyncEvent>>) {
        let (tx, rx) = broadcast::channel(128);
        (
            Self {
                event_tx: tx,
                behaviour,
            },
            rx,
        )
    }
}

#[derive(Clone, Debug)]
#[allow(unused)]
enum DummySyncEvent {
    SessionCreated,
    SyncStarted,
    Received(DummySyncMessage),
    SyncFinished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum DummySyncMessage {
    Data,
    Close,
}

#[derive(Debug)]
struct DummySyncManager<C, P> {
    pub event_tx: broadcast::Sender<FromSync<DummySyncEvent>>,
    #[allow(unused)]
    pub event_rx: broadcast::Receiver<FromSync<DummySyncEvent>>,
    pub config: C,
    pub _marker: PhantomData<P>,
}

impl SyncManagerTrait<TopicId> for DummySyncManager<FailingSyncConfig, FailingSyncProtocol> {
    type Protocol = FailingSyncProtocol;
    type Event = DummySyncEvent;
    type Config = FailingSyncConfig;
    type Message = ();
    type Error = SendError;

    fn from_config(config: Self::Config) -> Self {
        let event_rx = config.event_tx.subscribe();
        DummySyncManager {
            event_tx: config.event_tx.clone(),
            event_rx,
            config,
            _marker: PhantomData,
        }
    }

    async fn session(
        &mut self,
        session_id: u64,
        config: &p2panda_sync::SyncSessionConfig<TopicId>,
    ) -> Self::Protocol {
        self.event_tx
            .send(FromSync {
                session_id,
                remote: config.remote,
                event: DummySyncEvent::SessionCreated,
            })
            .unwrap();
        FailingSyncProtocol {
            behaviour: self.config.behaviour.clone(),
        }
    }

    async fn session_handle(
        &self,
        _session_id: u64,
    ) -> Option<std::pin::Pin<Box<dyn Sink<ToSync<Self::Message>, Error = Self::Error>>>> {
        // Just a dummy channel to satisfy the API in testing environment.
        let (tx, _) = mpsc::channel::<ToSync<Self::Message>>(128);
        let sink = Box::pin(tx) as Pin<Box<dyn Sink<ToSync<Self::Message>, Error = Self::Error>>>;
        Some(sink)
    }

    fn subscribe(&mut self) -> impl Stream<Item = FromSync<Self::Event>> + Send + Unpin + 'static {
        let stream = BroadcastStream::new(self.event_tx.subscribe())
            .filter_map(|event| async { event.ok() });
        Box::pin(stream)
    }
}

#[tokio::test]
async fn failed_sync_session_retry() {
    setup_logging();

    let topic = [0; 32];

    for (alice_behavior, bob_behavior) in [
        (SyncBehaviour::Panic, SyncBehaviour::Wait),
        (SyncBehaviour::Wait, SyncBehaviour::Panic),
        (SyncBehaviour::Error, SyncBehaviour::Wait),
        (SyncBehaviour::Wait, SyncBehaviour::Error),
        (SyncBehaviour::Error, SyncBehaviour::Error),
    ] {
        // Spawn nodes.
        let (bob_sync_config, _bob_rx) = FailingSyncConfig::new(bob_behavior);
        let mut bob = FailingNode::spawn(random(), vec![], bob_sync_config).await;

        let (alice_sync_config, _alice_rx) = FailingSyncConfig::new(alice_behavior);
        let alice =
            FailingNode::spawn(random(), vec![bob.args.node_info()], alice_sync_config).await;

        // Alice and Bob create stream for the same topic.
        let alice_handle = {
            let manager_ref = call!(alice.sync_ref, ToSyncManager::Create, topic, true).unwrap();
            SyncHandle::new(topic, alice.sync_ref.clone(), manager_ref)
        };
        let mut alice_subscription = alice_handle.subscribe().await.unwrap();

        let _bob_handle = {
            let manager_ref = call!(bob.sync_ref, ToSyncManager::Create, topic, true).unwrap();
            SyncHandle::new(topic, bob.sync_ref.clone(), manager_ref)
        };

        // Alice manually initiates a sync session with Bob.
        alice_handle.initiate_session(bob.node_id());

        let event = alice_subscription.next().await.unwrap();
        let expected_remote = bob.node_id();
        assert!(
            matches!(
                event,
                Ok(FromSync {
                    session_id: 0,
                    remote,
                    event: DummySyncEvent::SessionCreated
                }) if remote == expected_remote
            ),
            "{:#?}",
            event
        );
        let event = alice_subscription.next().await.unwrap();
        assert!(
            matches!(
                event,
                Ok(FromSync {
                    session_id: 1,
                    remote,
                    event: DummySyncEvent::SessionCreated
                }) if remote == expected_remote
            ),
            "{:#?}",
            event
        );

        alice.shutdown();
        bob.shutdown();
    }
}
