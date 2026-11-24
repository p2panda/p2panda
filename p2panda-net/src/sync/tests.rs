// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::test_utils::setup_logging;
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, Topic};
use p2panda_sync::traits::Protocol;
use p2panda_sync::{FromSync, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use rand::random;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::NodeId;
use crate::address_book::AddressBook;
use crate::addrs::NodeInfo;
use crate::gossip::Gossip;
use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::{SyncManager, ToSyncManager};
use crate::sync::authoriser::SyncBlockList;
use crate::sync::handle::{SyncHandle, SyncHandleError};
use crate::sync::hooks::SyncHooksList;
use crate::sync::session::{SessionBuilder, SyncSessionItem};
use crate::test_utils::{ApplicationArguments, test_args_from_seed};

const TEST_PROTOCOL_ID: [u8; 32] = [101; 32];

struct FailingNode {
    args: ApplicationArguments,
    sync_ref: ActorRef<ToSyncManager<DummySyncMessage, DummySyncEvent>>,
}

impl FailingNode {
    pub async fn spawn(
        seed: [u8; 32],
        node_infos: Vec<NodeInfo>,
        session_builder: FailingSessionBuilder,
    ) -> Self {
        let args = test_args_from_seed(seed);

        let address_book = AddressBook::builder().spawn().await.unwrap();

        // Pre-populate the address book with known addresses.
        for info in node_infos {
            address_book.insert_node_info(info).await.unwrap();
        }

        let endpoint = Endpoint::builder(address_book.clone())
            .config(args.iroh_config.clone())
            .signing_key(args.signing_key.clone())
            .spawn()
            .await
            .unwrap();

        let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
            .spawn()
            .await
            .unwrap();

        let thread_pool = ThreadLocalActorSpawner::new();

        let mut hooks = SyncHooksList::new();
        hooks.push(SyncBlockList::default());

        let (sync_ref, _) = SyncManager::<FailingSessionBuilder>::spawn(
            None,
            (
                TEST_PROTOCOL_ID.to_vec(),
                session_builder,
                endpoint,
                gossip,
                hooks,
            ),
            thread_pool,
        )
        .await
        .unwrap();

        Self { args, sync_ref }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.verifying_key
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
    _events: mpsc::Sender<DummySyncEvent>,
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
struct FailingSessionBuilder {
    pub behaviour: SyncBehaviour,
}

impl FailingSessionBuilder {
    pub fn new(behaviour: SyncBehaviour) -> Self {
        Self { behaviour }
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

impl Digest<Hash> for DummySyncMessage {
    fn hash(&self) -> Hash {
        Hash::digest(format!("{self:?}"))
    }
}

impl SessionBuilder for FailingSessionBuilder {
    type Protocol = FailingSyncProtocol;
    type Operation = DummySyncMessage;
    type Event = DummySyncEvent;

    fn build(
        &self,
        _topic: Topic,
        _live_rx: Option<mpsc::Receiver<ToSync<Self::Operation>>>,
        events: mpsc::Sender<Self::Event>,
    ) -> Self::Protocol {
        events.try_send(DummySyncEvent::SessionCreated).unwrap();
        FailingSyncProtocol {
            behaviour: self.behaviour.clone(),
            _events: events,
        }
    }
}

impl SyncSessionItem<DummySyncMessage> for DummySyncEvent {
    fn operation(&self) -> Option<&DummySyncMessage> {
        match self {
            DummySyncEvent::Received(message) => Some(message),
            _ => None,
        }
    }
}

#[tokio::test]
async fn failed_sync_session_retry() {
    setup_logging();

    let topic = [0; 32].into();

    for (alice_behavior, bob_behavior) in [
        (SyncBehaviour::Panic, SyncBehaviour::Wait),
        (SyncBehaviour::Wait, SyncBehaviour::Panic),
        (SyncBehaviour::Error, SyncBehaviour::Wait),
        (SyncBehaviour::Wait, SyncBehaviour::Error),
        (SyncBehaviour::Error, SyncBehaviour::Error),
    ] {
        // Spawn nodes.
        let bob_sync_config = FailingSessionBuilder::new(bob_behavior);
        let mut bob = FailingNode::spawn(random(), vec![], bob_sync_config).await;

        let alice_sync_config = FailingSessionBuilder::new(alice_behavior);
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

#[tokio::test]
async fn shutdown_handles() {
    setup_logging();

    let topic = [1; 32].into();
    let node = FailingNode::spawn(
        random(),
        vec![],
        FailingSessionBuilder::new(SyncBehaviour::Wait),
    )
    .await;

    let handle_1 = {
        let manager_ref = call!(node.sync_ref, ToSyncManager::Create, topic, true).unwrap();
        SyncHandle::new(topic, node.sync_ref.clone(), manager_ref)
    };
    let handle_2 = {
        let manager_ref = call!(node.sync_ref, ToSyncManager::Create, topic, true).unwrap();
        SyncHandle::new(topic, node.sync_ref.clone(), manager_ref)
    };

    drop(handle_1);
    assert!(handle_2.subscribe().await.is_ok());

    // Closing one handle doesn't affect the others.
    let handle_3 = {
        let manager_ref = call!(node.sync_ref, ToSyncManager::Create, topic, true).unwrap();
        SyncHandle::new(topic, node.sync_ref.clone(), manager_ref)
    };
    handle_2.close().await.unwrap();
    assert!(handle_3.subscribe().await.is_ok());

    // Closed handles can't be used anymore.
    assert!(matches!(
        handle_2.subscribe().await,
        Err(SyncHandleError::Closed)
    ));
    assert!(matches!(
        handle_2.publish(DummySyncMessage::Data),
        Err(SyncHandleError::Closed)
    ));

    // Closing the last handle closes the topic.
    handle_3.close().await.unwrap();
    assert!(handle_3.subscribe().await.is_err());

    // Stale handles don't attach to a new topic manager for the same topic.
    let _handle_4 = {
        let manager_ref = call!(node.sync_ref, ToSyncManager::Create, topic, true).unwrap();
        SyncHandle::new(topic, node.sync_ref.clone(), manager_ref)
    };
    assert!(handle_3.subscribe().await.is_err());

    node.shutdown();
}
