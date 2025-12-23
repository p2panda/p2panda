// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Duration;

use assert_matches::assert_matches;
use futures_channel::mpsc::{self, SendError};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::{Body, Operation};
use p2panda_discovery::address_book::AddressBookStore as _;
use p2panda_sync::topic_log_sync::TopicLogSyncEvent as Event;
use p2panda_sync::traits::{Protocol, SyncManager};
use p2panda_sync::{FromSync, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorRef, call};
use rand::random;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::gossip::{GOSSIP, Gossip, ToGossip};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, EventuallyConsistentStreams, ToEventuallyConsistentStreams,
};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::test_utils::{
    App, ApplicationArguments, DummySyncConfig, DummySyncEvent, DummySyncManager, DummySyncMessage,
    TestTopicSyncManager, generate_node_info, setup_logging, test_args_from_seed,
};

struct TestNode<M>
where
    M: SyncManager<TopicId> + Debug + Send + 'static,
{
    args: ApplicationArguments,
    gossip_ref: ActorRef<ToGossip>,
    endpoint_ref: ActorRef<ToIrohEndpoint>,
    address_book_ref: ActorRef<ToAddressBook>,
    stream_ref: ActorRef<ToEventuallyConsistentStreams<M>>,
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
        let (address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (args.clone(), store.clone()),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the endpoint actor.
        let (endpoint_ref, _) = IrohEndpoint::spawn(
            Some(with_namespace(IROH_ENDPOINT, &actor_namespace)),
            args.clone(),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        let endpoint = call!(endpoint_ref, ToIrohEndpoint::Endpoint).unwrap();

        // Spawn the gossip actor.
        let (gossip_ref, _) = Gossip::<M>::spawn(
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
            (args.clone(), gossip_ref.clone(), sync_config.clone()),
            args.root_thread_pool.clone(),
        )
        .await
        .unwrap();

        Self {
            args,
            gossip_ref,
            endpoint_ref,
            address_book_ref,
            stream_ref,
            thread_pool,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub fn shutdown(&self) {
        self.gossip_ref.stop(None);
        self.endpoint_ref.stop(None);
        self.stream_ref.stop(None);
        self.address_book_ref.stop(None);
    }
}

#[tokio::test]
async fn e2e_no_sync() {
    setup_logging();
    let topic_id = [0; 32];

    let (bob_sync_config, _bob_rx) = DummySyncConfig::new();
    let mut bob: TestNode<DummySyncManager> =
        TestNode::spawn([11; 32], vec![], bob_sync_config).await;

    let (alice_sync_config, _alice_rx) = DummySyncConfig::new();
    let alice: TestNode<DummySyncManager> = TestNode::spawn(
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
                event: DummySyncEvent::SessionCreated
            } if remote == expected_remote
        ));
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: DummySyncEvent::SyncStarted
            } if remote == expected_remote
        ));
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: DummySyncEvent::Received(DummySyncMessage::Data)
            } if remote == expected_remote
        ));
        assert!(matches!(
            sub.recv().await.unwrap(),
            FromSync {
                session_id: 0,
                remote,
                event: DummySyncEvent::SyncFinished
            } if remote == expected_remote
        ));
    }

    alice.shutdown();
    bob.shutdown();
}

#[tokio::test]
async fn e2e_topic_log_sync() {
    setup_logging();
    const TOPIC_ID: [u8; 32] = [0; 32];
    const LOG_ID: u64 = 0;

    // Setup Alice's app logic.
    let mut alice_app = {
        // @NOTE: In these tests the "app" layer where the p2panda identity lives has a different
        // private key from the node.
        let mut app = App::new(0);
        let body = Body::new(b"Hello from Alice");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Bob's app logic.
    let bob_app = {
        let mut app = App::new(1);
        let body = Body::new(b"Hello from Bob");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Bob's node.
    let mut bob: TestNode<TestTopicSyncManager> =
        TestNode::spawn([13; 32], vec![], bob_app.sync_config()).await;

    // Setup Alice's node.
    let alice: TestNode<TestTopicSyncManager> = TestNode::spawn(
        [12; 32],
        vec![generate_node_info(&mut bob.args)],
        alice_app.sync_config(),
    )
    .await;

    // Create Alice's stream.
    let alice_stream = call!(
        alice.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Create Bob's stream.
    let bob_stream = call!(
        bob.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Subscribe to both streams.
    let mut alice_subscription = alice_stream.subscribe().await.unwrap();
    let mut bob_subscription = bob_stream.subscribe().await.unwrap();

    // Alice initiates sync.
    alice
        .stream_ref
        .cast(ToEventuallyConsistentStreams::InitiateSync(
            TOPIC_ID,
            bob.node_id(),
        ))
        .unwrap();

    // Assert Alice receives the expected events.
    let bob_id = bob.node_id();
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        } if remote == bob_id
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncFinished(_),
            ..
        }
    );
    let event: FromSync<Event<()>> = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeStarted,
            ..
        }
    );

    // Assert Bob receives the expected events.
    let alice_id = alice.node_id();
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        } if remote == alice_id
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncFinished(_),
            ..
        }
    );
    let event: FromSync<Event<()>> = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeStarted,
            ..
        }
    );

    // Alice publishes a live mode message.
    let body = Body::new(b"live message from alice");
    let (header, _) = alice_app.create_operation(&body, LOG_ID).await;
    alice_stream
        .publish(Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        })
        .await
        .unwrap();

    // Bob receives Alice's message.
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );

    tokio::time::sleep(Duration::from_millis(500)).await;

    alice_stream.close().unwrap();

    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeFinished(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Success,
            ..
        }
    );

    // Assert Alice's final events.
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeFinished(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Success,
            ..
        }
    );

    alice.shutdown();
    bob.shutdown();
}

#[tokio::test]
async fn e2e_three_party_sync() {
    setup_logging();
    const TOPIC_ID: [u8; 32] = [0; 32];
    const LOG_ID: u64 = 0;

    // Setup Alice's app logic.
    let mut alice_app = {
        // @NOTE: In these tests the "app" layer where the p2panda identity lives has a different
        // private key from the node.
        let mut app = App::new(0);
        let body = Body::new(b"Hello from Alice");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Bob's app logic.
    let bob_app = {
        let mut app = App::new(1);
        let body = Body::new(b"Hello from Bob");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Carol's app logic.
    let carol_app = {
        let mut app = App::new(2);
        let body = Body::new(b"Hello from Carol");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Bob's node.
    let mut bob: TestNode<TestTopicSyncManager> =
        TestNode::spawn([30; 32], vec![], bob_app.sync_config()).await;

    // Setup Alice's node.
    let mut alice: TestNode<TestTopicSyncManager> = TestNode::spawn(
        [31; 32],
        vec![generate_node_info(&mut bob.args)],
        alice_app.sync_config(),
    )
    .await;

    // Setup Alice's node.
    let carol: TestNode<TestTopicSyncManager> = TestNode::spawn(
        [32; 32],
        vec![generate_node_info(&mut alice.args)],
        carol_app.sync_config(),
    )
    .await;

    // Create Alice's stream.
    let alice_stream = call!(
        alice.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Create Bob's stream.
    let bob_stream = call!(
        bob.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Subscribe to both streams.
    let mut alice_subscription = alice_stream.subscribe().await.unwrap();
    let mut bob_subscription = bob_stream.subscribe().await.unwrap();

    // Alice initiates sync.
    alice
        .stream_ref
        .cast(ToEventuallyConsistentStreams::InitiateSync(
            TOPIC_ID,
            bob.node_id(),
        ))
        .unwrap();

    // Assert Alice receives the expected events.
    let bob_id = bob.node_id();
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        } if remote == bob_id
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncFinished(_),
            ..
        }
    );
    let event: FromSync<Event<()>> = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeStarted,
            ..
        }
    );

    // Assert Bob receives the expected events.
    let alice_id = alice.node_id();
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            session_id: 0,
            remote,
            event: Event::SyncStarted(_),
        } if remote == alice_id
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncFinished(_),
            ..
        }
    );
    let event: FromSync<Event<()>> = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeStarted,
            ..
        }
    );

    // Alice publishes a live mode message.
    let body = Body::new(b"live message from alice");
    let (header, _) = alice_app.create_operation(&body, LOG_ID).await;
    alice_stream
        .publish(Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        })
        .await
        .unwrap();

    // Bob receives Alice's message.
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );

    // Create Carol's stream.
    let carol_stream = call!(
        carol.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Carol initiates sync with Alice.
    carol
        .stream_ref
        .cast(ToEventuallyConsistentStreams::InitiateSync(
            TOPIC_ID,
            alice.node_id(),
        ))
        .unwrap();

    let mut carol_subscription = carol_stream.subscribe().await.unwrap();

    let event = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            session_id: 0,
            event: Event::SyncStarted(_),
            ..
        }
    );
    let event = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStatus(_),
            ..
        }
    );
    let event = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );
    let event = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Operation(_),
            ..
        }
    );
    let event = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncFinished(_),
            ..
        }
    );
    let event: FromSync<Event<()>> = carol_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeStarted,
            ..
        }
    );
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("unexpected sync failure")]
    UnexpectedFailure,
}

#[derive(Debug, Clone)]
pub enum SyncBehaviour {
    Panic,
    Error,
    Wait,
}

#[derive(Debug)]
pub struct FailingSyncProtocol {
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
        // Send one message otherwise the accepting peer will not be able to accept the
        // connection.
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
pub struct FailingSyncConfig {
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

impl SyncManager<TopicId> for DummySyncManager<FailingSyncConfig, FailingSyncProtocol> {
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
            _phantom: PhantomData,
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
        // NOTE: just a dummy channel to satisfy the API in testing environment.
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
    let topic_id = [0; 32];

    for (alice_behavior, bob_behavior) in [
        (SyncBehaviour::Panic, SyncBehaviour::Wait),
        (SyncBehaviour::Wait, SyncBehaviour::Panic),
        (SyncBehaviour::Error, SyncBehaviour::Wait),
        (SyncBehaviour::Wait, SyncBehaviour::Error),
        (SyncBehaviour::Error, SyncBehaviour::Error),
    ] {
        let (bob_sync_config, _bob_rx) = FailingSyncConfig::new(bob_behavior);
        let mut bob: TestNode<DummySyncManager<FailingSyncConfig, FailingSyncProtocol>> =
            TestNode::spawn(random(), vec![], bob_sync_config).await;

        let (alice_sync_config, _alice_rx) = FailingSyncConfig::new(alice_behavior);
        let alice: TestNode<DummySyncManager<FailingSyncConfig, FailingSyncProtocol>> =
            TestNode::spawn(
                random(),
                vec![generate_node_info(&mut bob.args)],
                alice_sync_config,
            )
            .await;

        let alice_stream = call!(
            alice.stream_ref,
            ToEventuallyConsistentStreams::Create,
            topic_id,
            true
        )
        .unwrap();
        let mut alice_subscription = alice_stream.subscribe().await.unwrap();

        let _bob_stream = call!(
            bob.stream_ref,
            ToEventuallyConsistentStreams::Create,
            topic_id,
            true
        )
        .unwrap();

        alice
            .stream_ref
            .cast(ToEventuallyConsistentStreams::InitiateSync(
                topic_id,
                bob.node_id(),
            ))
            .unwrap();

        let event = alice_subscription.recv().await.unwrap();
        let expected_remote = bob.node_id();
        assert!(
            matches!(
                event,
                FromSync {
                    session_id: 0,
                    remote,
                    event: DummySyncEvent::SessionCreated
                } if remote == expected_remote
            ),
            "{:#?}",
            event
        );
        let event = alice_subscription.recv().await.unwrap();
        assert!(
            matches!(
                event,
                FromSync {
                    session_id: 1,
                    remote,
                    event: DummySyncEvent::SessionCreated
                } if remote == expected_remote
            ),
            "{:#?}",
            event
        );

        alice.shutdown();
        bob.shutdown();
    }
}

#[tokio::test]
async fn topic_log_sync_failure_and_retry() {
    setup_logging();
    const TOPIC_ID: [u8; 32] = [0; 32];
    const LOG_ID: u64 = 0;

    // Setup Alice's app logic.
    let alice_app = {
        // @NOTE: In these tests the "app" layer where the p2panda identity lives has a different
        // private key from the node.
        let mut app = App::new(0);
        let body = Body::new(b"Hello from Alice");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Bob's app logic.
    let bob_app = {
        let mut app = App::new(1);
        let body = Body::new(b"Hello from Bob");
        let _ = app.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(app.id(), vec![LOG_ID])]);
        app.insert_topic(&TOPIC_ID, &logs);
        app
    };

    // Setup Bob's node.

    // Setup Alice's node.
    let alice_seed = [16; 32];
    let mut alice: TestNode<TestTopicSyncManager> =
        TestNode::spawn(alice_seed, vec![], alice_app.sync_config()).await;

    let mut bob: TestNode<TestTopicSyncManager> = TestNode::spawn(
        [15; 32],
        vec![generate_node_info(&mut alice.args)],
        bob_app.sync_config(),
    )
    .await;

    // Create Alice's stream.
    let alice_stream = call!(
        alice.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Create Bob's stream.
    let bob_stream = call!(
        bob.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();

    // Subscribe to both streams.
    let mut alice_subscription = alice_stream.subscribe().await.unwrap();
    let mut bob_subscription = bob_stream.subscribe().await.unwrap();

    // Bob initiates sync.
    bob.stream_ref
        .cast(ToEventuallyConsistentStreams::InitiateSync(
            TOPIC_ID,
            alice.node_id(),
        ))
        .unwrap();

    // Drain alice's event stream.
    for _ in 0..6 {
        alice_subscription.recv().await.unwrap();
    }

    // Drain bob's event stream.
    for _ in 0..6 {
        bob_subscription.recv().await.unwrap();
    }

    // Alice unexpectedly shuts down.
    alice.shutdown();

    // Bob is informed that the session failed.
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::LiveModeFinished(_),
            ..
        }
    );
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::Failed { .. },
            ..
        }
    );

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Alice starts up their node again and subscribes to the same topic.
    let alice: TestNode<TestTopicSyncManager> = TestNode::spawn(
        alice_seed,
        vec![generate_node_info(&mut bob.args)],
        alice_app.sync_config(),
    )
    .await;
    let alice_stream = call!(
        alice.stream_ref,
        ToEventuallyConsistentStreams::Create,
        TOPIC_ID,
        true
    )
    .unwrap();
    let mut alice_subscription = alice_stream.subscribe().await.unwrap();

    // Bob should automatically attempt restart and therefore both peers get a "sync started"
    // event.
    let event = bob_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStarted(_),
            ..
        }
    );
    let event = alice_subscription.recv().await.unwrap();
    assert_matches!(
        event,
        FromSync {
            event: Event::SyncStarted(_),
            ..
        }
    );

    alice.shutdown();
    bob.shutdown();
}
