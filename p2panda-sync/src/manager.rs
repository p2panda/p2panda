// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::ready;
use futures::stream::SelectAll;
use futures::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::{Extensions, Operation, PublicKey};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::debug;

use crate::map::SessionTopicMap;
use crate::protocol::{TopicLogSync, TopicLogSyncError, TopicLogSyncEvent};
use crate::traits::{SyncManager, TopicLogMap};
use crate::{FromSync, SyncSessionConfig, ToSync};

static CHANNEL_BUFFER: usize = 1028;

pub trait StreamDebug<Item>: Stream<Item = Item> + Send + Debug + 'static {}

impl<T, Item> StreamDebug<Item> for T where T: Stream<Item = Item> + Send + Debug + 'static {}

/// Create and manage topic log sync sessions.
///
/// Sync sessions are created via the manager, which instantiates them with access to the shared
/// topic map and operation store as well as channels for receiving sync events and for sending
/// newly arriving operations in live mode.
///
/// The manager method `next_event` must be polled in order to consume events coming from any
/// running sync sessions, as well as to allow the manager to perform important event forwarding
/// between sync sessions.
///
/// A handle can be acquired to a sync session via the session_handle method for sending any live
/// mode operations to a specific session. It's expected that users map sessions (by their id) to
/// any topic subscriptions in order to understand the correct mappings.  
#[allow(clippy::type_complexity)]
#[derive(Debug)]
pub struct TopicSyncManager<T, S, M, L, E>
where
    T: Clone,
    E: Extensions,
{
    topic_map: M,
    store: S,
    session_topic_map: SessionTopicMap<T, mpsc::Sender<ToSync<Operation<E>>>>,
    from_session_tx: HashMap<(u64, PublicKey), broadcast::Sender<TopicLogSyncEvent<E>>>,
    from_session_rx: HashMap<(u64, PublicKey), broadcast::Receiver<TopicLogSyncEvent<E>>>,
    manager_tx: Vec<mpsc::Sender<SessionStream<T, E>>>,
    _phantom: PhantomData<L>,
}

#[derive(Debug)]
struct SessionStream<T, E>
where
    T: Clone,
    E: Extensions,
{
    pub session_id: u64,
    pub topic: T,
    pub remote: PublicKey,
    pub event_rx: broadcast::Receiver<TopicLogSyncEvent<E>>,
    pub live_tx: mpsc::Sender<ToSync<Operation<E>>>,
}

impl<T, S, M, L, E> TopicSyncManager<T, S, M, L, E>
where
    T: Clone,
    E: Extensions,
{
    pub fn new(topic_map: M, store: S) -> Self {
        Self {
            topic_map,
            store,
            manager_tx: Default::default(),
            session_topic_map: Default::default(),
            from_session_tx: Default::default(),
            from_session_rx: Default::default(),
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> SyncManager<T> for TopicSyncManager<T, S, M, L, E>
where
    T: Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    S: LogStore<L, E> + OperationStore<L, E> + Send + 'static,
    M: TopicLogMap<T, L> + Send + 'static,
    L: LogId + for<'de> Deserialize<'de> + Serialize + Send + 'static,
    E: Extensions + Send + 'static,
{
    type Protocol = TopicLogSync<T, S, M, L, E>;
    type Config = TopicSyncManagerConfig<S, M>;
    type Event = TopicLogSyncEvent<E>;
    type Message = Operation<E>;
    type Error = TopicSyncManagerError;

    fn from_config(config: Self::Config) -> Self {
        Self::new(config.topic_map, config.store)
    }

    async fn session(&mut self, session_id: u64, config: &SyncSessionConfig<T>) -> Self::Protocol {
        let (live_tx, live_rx) = mpsc::channel(CHANNEL_BUFFER);
        let (event_tx, event_rx) = broadcast::channel::<TopicLogSyncEvent<E>>(CHANNEL_BUFFER);

        self.from_session_tx
            .insert((session_id, config.remote), event_tx.clone());

        self.from_session_rx
            .insert((session_id, config.remote), event_rx);

        self.session_topic_map
            .insert_with_topic(session_id, config.topic.clone(), live_tx.clone());

        for manager_tx in self.manager_tx.iter_mut() {
            if manager_tx
                .send(SessionStream {
                    session_id,
                    topic: config.topic.clone(),
                    remote: config.remote,
                    event_rx: event_tx.subscribe(),
                    live_tx: live_tx.clone(),
                })
                .await
                .is_err()
            {
                debug!("manager handle dropped");
            };
        }

        let live_rx = if config.live_mode {
            Some(live_rx)
        } else {
            None
        };

        TopicLogSync::new(
            config.topic.clone(),
            self.store.clone(),
            self.topic_map.clone(),
            live_rx,
            event_tx,
        )
    }

    async fn session_handle(
        &self,
        session_id: u64,
    ) -> Option<Pin<Box<dyn Sink<ToSync<Operation<E>>, Error = Self::Error>>>> {
        let map_fn = |to_sync: ToSync<Operation<E>>| {
            ready({
                match to_sync {
                    ToSync::Payload(operation) => Ok::<_, Self::Error>(ToSync::Payload(operation)),
                    ToSync::Close => Ok::<_, Self::Error>(ToSync::Close),
                }
            })
        };

        self.session_topic_map.sender(session_id).map(|tx| {
            Box::pin(tx.clone().with(map_fn))
                as Pin<Box<dyn Sink<ToSync<Operation<E>>, Error = Self::Error>>>
        })
    }

    fn subscribe(&mut self) -> impl Stream<Item = FromSync<Self::Event>> + Send + Unpin + 'static {
        let (manager_tx, manager_rx) = mpsc::channel(CHANNEL_BUFFER);
        self.manager_tx.push(manager_tx);

        let mut session_rx_set = SelectAll::new();
        for ((id, remote), tx) in self.from_session_tx.iter() {
            let session_id = *id;
            let remote = *remote;
            let stream = BroadcastStream::new(tx.subscribe());

            #[allow(clippy::type_complexity)]
            let stream: Pin<
                Box<dyn StreamDebug<Option<FromSync<TopicLogSyncEvent<E>>>>>,
            > = Box::pin(stream.map(Box::new(
                move |event: Result<TopicLogSyncEvent<E>, BroadcastStreamRecvError>| {
                    event.ok().map(|event| FromSync {
                        session_id,
                        remote,
                        event,
                    })
                },
            )));
            session_rx_set.push(stream);
        }

        let state = ManagerEventStreamState {
            manager_rx,
            session_rx_set,
            session_topic_map: self.session_topic_map.clone(),
        };

        let stream = ManagerEventStream {
            state: Some(state),
            pending: Default::default(),
        };

        Box::pin(stream)
    }
}

#[allow(clippy::type_complexity)]
pub struct ManagerEventStreamState<T, E>
where
    T: Clone + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    E: Extensions + Send + 'static,
{
    manager_rx: mpsc::Receiver<SessionStream<T, E>>,
    session_rx_set: SelectAll<Pin<Box<dyn StreamDebug<Option<FromSync<TopicLogSyncEvent<E>>>>>>>,
    session_topic_map: SessionTopicMap<T, mpsc::Sender<ToSync<Operation<E>>>>,
}

/// Event stream for a manager returned from SyncManager::subscribe().
///
/// This stream must be polled in order for the manager to forward live mode events onto
/// concurrently running sync sessions.
#[allow(clippy::type_complexity)]
pub struct ManagerEventStream<T, E>
where
    T: Clone + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    E: Extensions + Send + 'static,
{
    state: Option<ManagerEventStreamState<T, E>>,

    /// The current future being polled.
    pending: Option<
        Pin<
            Box<
                dyn Future<
                        Output = (
                            ManagerEventStreamState<T, E>,
                            Option<FromSync<TopicLogSyncEvent<E>>>,
                        ),
                    > + Send,
            >,
        >,
    >,
}

impl<T, E> ManagerEventStream<T, E>
where
    T: Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    E: Extensions + Send + 'static,
{
    async fn next_event(
        mut state: ManagerEventStreamState<T, E>,
    ) -> (
        ManagerEventStreamState<T, E>,
        Option<FromSync<TopicLogSyncEvent<E>>>,
    ) {
        loop {
            tokio::select!(
                biased;
                item = state.manager_rx.next() => {
                    let Some(manager_event) = item else {
                        debug!("manager event stream closed");
                        return (state, None)
                    };
                    debug!("manager event received: {manager_event:?}");
                    let session_id = manager_event.session_id;
                    state.session_topic_map
                    .insert_with_topic(session_id, manager_event.topic, manager_event.live_tx);

                    let stream = BroadcastStream::new(manager_event.event_rx);

                    #[allow(clippy::type_complexity)]
                    let stream: Pin<Box<dyn StreamDebug<Option<FromSync<TopicLogSyncEvent<E>>>>>> =
                        Box::pin(stream.map(Box::new(
                            move |event: Result<TopicLogSyncEvent<E>, BroadcastStreamRecvError>| {
                                event.ok().map(|event| FromSync {
                                    session_id,
                                    remote: manager_event.remote,
                                    event,
                                })
                            },
                        )));
                    state.session_rx_set.push(stream);
                }
                Some(Some(from_sync)) = state.session_rx_set.next() => {
                    debug!("from sync event received: {from_sync:?}");
                    let session_id = from_sync.session_id();
                    let event = from_sync.event();

                    let operation = match event {
                        TopicLogSyncEvent::Operation(operation) => Some(*operation.clone()),
                        _ => return (state, Some(from_sync)),
                    };

                    if let Some(operation) = operation {
                        let Some(topic) = state.session_topic_map.topic(session_id) else {
                            debug!("session {session_id} not found");
                            state.session_topic_map.drop(session_id);
                            continue;
                        };
                        let keys = state.session_topic_map.sessions(topic);
                        let mut dropped = vec![];

                        for id in keys {
                            if id == session_id {
                                continue;
                            }

                            let Some(tx) = state.session_topic_map.sender_mut(id) else {
                                debug!("session {id} channel unexpectedly closed");
                                state.session_topic_map.drop(session_id);
                                continue;
                            };

                            let result = tx.send(ToSync::Payload(operation.clone())).await;

                            if result.is_err() {
                                debug!("failed sending message on session channel");
                                dropped.push(id);
                            }
                        }

                        for id in dropped {
                            state.session_topic_map.drop(id);
                        }
                    }

                    return (state, Some(from_sync))
                }
            )
        }
    }
}

impl<T, E> Unpin for ManagerEventStream<T, E>
where
    T: Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    E: Extensions + Send + 'static,
{
}

impl<T, E> Stream for ManagerEventStream<T, E>
where
    T: Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    E: Extensions + Send + 'static,
{
    type Item = FromSync<TopicLogSyncEvent<E>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.pending.is_none() {
            let fut = Box::pin(ManagerEventStream::next_event(
                self.state.take().expect("state is not None"),
            ));
            self.pending = Some(fut);
        }

        let fut = self.pending.as_mut().unwrap();
        match fut.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready((state, item)) => {
                self.pending = None;
                self.state.replace(state);
                Poll::Ready(item)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TopicSyncManagerConfig<S, M> {
    pub store: S,
    pub topic_map: M,
}

#[derive(Debug, Error)]
pub enum TopicSyncManagerError {
    #[error(transparent)]
    TopicLogSync(#[from] TopicLogSyncError),

    #[error("received operation before topic agreed")]
    OperationBeforeTopic,

    #[error(transparent)]
    Send(#[from] mpsc::SendError),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use assert_matches::assert_matches;
    use futures::{SinkExt, StreamExt};
    use p2panda_core::{Body, Operation};

    use crate::TopicSyncManager;
    use crate::manager::TopicSyncManagerConfig;
    use crate::protocol::TopicLogSyncEvent;
    use crate::test_utils::{
        Peer, TestMemoryStore, TestTopic, TestTopicMap, TestTopicSyncEvent, TestTopicSyncManager,
        drain_stream, run_protocol, setup_logging,
    };
    use crate::traits::SyncManager;
    use crate::{FromSync, SyncSessionConfig, ToSync};

    #[test]
    fn from_config() {
        let store = TestMemoryStore::new();
        let topic_map = TestTopicMap::new();
        let config = TopicSyncManagerConfig { store, topic_map };
        let _: TestTopicSyncManager = SyncManager::from_config(config);
    }

    #[tokio::test]
    async fn manager_e2e() {
        setup_logging();

        const TOPIC_NAME: &str = "messages";
        const LOG_ID: u64 = 0;
        const SESSION_ID: u64 = 0;

        let topic = TestTopic::new(TOPIC_NAME);

        // Setup Peer A
        let mut peer_a = Peer::new(0);
        let body = Body::new("Hello from Peer A".as_bytes());
        let _ = peer_a.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(peer_a.id(), vec![LOG_ID])]);
        peer_a.insert_topic(&topic, &logs);
        let mut peer_a_manager =
            TopicSyncManager::new(peer_a.topic_map.clone(), peer_a.store.clone());

        // Setup Peer B
        let mut peer_b = Peer::new(1);
        let body = Body::new("Hello from Peer B".as_bytes());
        let _ = peer_b.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(peer_b.id(), vec![LOG_ID])]);
        peer_b.insert_topic(&topic, &logs);
        let mut peer_b_manager =
            TopicSyncManager::new(peer_b.topic_map.clone(), peer_b.store.clone());

        // Instantiate sync session for Peer A.
        let config = SyncSessionConfig {
            topic,
            remote: peer_b.id(),
            live_mode: true,
        };

        // Subscribe to both managers.
        let mut event_stream_a = peer_a_manager.subscribe();
        let mut event_stream_b = peer_b_manager.subscribe();

        // Instantiate sync session for Peer A.
        let peer_a_session = peer_a_manager.session(SESSION_ID, &config).await;

        // Instantiate sync session for Peer B.
        let peer_b_session = peer_b_manager.session(SESSION_ID, &config).await;

        // Get a handle to Peer A sync session.
        let mut peer_a_handle = peer_a_manager.session_handle(SESSION_ID).await.unwrap();

        // Create and send a new live-mode message.
        let (header_1, _) = peer_a.create_operation_no_insert(&body, LOG_ID).await;
        peer_a_handle
            .send(ToSync::Payload(Operation {
                hash: header_1.hash(),
                header: header_1.clone(),
                body: Some(body.clone()),
            }))
            .await
            .unwrap();
        peer_a_handle.send(ToSync::Close).await.unwrap();

        // Actually run the protocol.
        run_protocol(peer_a_session, peer_b_session).await.unwrap();

        // Assert Peer A's events.
        for index in 0..=7 {
            let event = event_stream_a.next().await.unwrap();
            assert_eq!(event.session_id(), 0);
            match index {
                0 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::SyncStarted(_),
                        ..
                    }
                ),
                1 | 2 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::SyncStatus(_),
                        ..
                    }
                ),
                3 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Operation(_),
                        ..
                    }
                ),
                4 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::SyncFinished(_),
                        ..
                    }
                ),
                5 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::LiveModeStarted,
                        ..
                    }
                ),
                6 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::LiveModeFinished(_),
                        ..
                    }
                ),
                7 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Success,
                        ..
                    }
                ),
                _ => panic!(),
            }
        }

        // Assert Peer B's events.
        for index in 0..=8 {
            let event = event_stream_b.next().await.unwrap();
            match index {
                0 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::SyncStarted(_),
                        ..
                    }
                ),
                1 | 2 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::SyncStatus(_),
                        ..
                    }
                ),
                3 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Operation(_),
                        ..
                    }
                ),
                4 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::SyncFinished(_),
                        ..
                    }
                ),
                5 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::LiveModeStarted,
                        ..
                    }
                ),
                6 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Operation(_),
                        ..
                    }
                ),
                7 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::LiveModeFinished(_),
                        ..
                    }
                ),
                8 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Success,
                        ..
                    }
                ),
                _ => panic!(),
            }
        }
    }

    #[tokio::test]
    async fn live_mode_three_peer_forwarding() {
        use std::collections::HashMap;

        const TOPIC_NAME: &str = "chat";
        const LOG_ID: u64 = 0;
        const SESSION_AB: u64 = 0;
        const SESSION_AC: u64 = 1;
        const SESSION_BA: u64 = 2;
        const SESSION_CA: u64 = 3;

        // Shared topic
        let topic = TestTopic::new(TOPIC_NAME);

        // Peer A
        let mut peer_a = Peer::new(0);
        let body_a = Body::new("Hello from A".as_bytes());
        let (peer_a_header_0, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let logs = HashMap::from([(peer_a.id(), vec![LOG_ID])]);
        peer_a.insert_topic(&topic, &logs);
        let mut manager_a = TopicSyncManager::new(peer_a.topic_map.clone(), peer_a.store.clone());

        // Peer B
        let mut peer_b = Peer::new(1);
        let body_b = Body::new("Hello from B".as_bytes());
        let (peer_b_header_0, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let logs = HashMap::from([(peer_b.id(), vec![LOG_ID])]);
        peer_b.insert_topic(&topic, &logs);
        let mut manager_b = TopicSyncManager::new(peer_b.topic_map.clone(), peer_b.store.clone());

        // Peer C
        let mut peer_c = Peer::new(2);
        let body_c = Body::new("Hello from C".as_bytes());
        let (peer_c_header_0, _) = peer_c.create_operation(&body_c, LOG_ID).await;
        let logs = HashMap::from([(peer_c.id(), vec![LOG_ID])]);
        peer_c.insert_topic(&topic, &logs);
        let mut manager_c = TopicSyncManager::new(peer_c.topic_map.clone(), peer_c.store.clone());

        // Session A -> B (A initiates)
        let mut config = SyncSessionConfig {
            topic: topic.clone(),
            remote: peer_b.id(),
            live_mode: true,
        };
        let session_ab = manager_a.session(SESSION_AB, &config).await;
        config.remote = peer_a.id();
        let session_b = manager_b.session(SESSION_BA, &config).await;

        // Session A -> C (A initiates)
        let mut config = SyncSessionConfig {
            topic: topic.clone(),
            remote: peer_c.id(),
            live_mode: true,
        };
        let session_ac = manager_a.session(SESSION_AC, &config).await;
        config.remote = peer_a.id();
        let session_c = manager_c.session(SESSION_CA, &config).await;

        let mut event_stream_a = manager_a.subscribe();
        let mut event_stream_b = manager_b.subscribe();
        let mut event_stream_c = manager_c.subscribe();

        // Run both protocols concurrently
        tokio::spawn(async move {
            run_protocol(session_ab, session_b).await.unwrap();
        });
        tokio::spawn(async move {
            run_protocol(session_ac, session_c).await.unwrap();
        });

        // Send live-mode messages from all peers
        let mut handle_ab = manager_a.session_handle(SESSION_AB).await.unwrap();
        let mut handle_ac = manager_a.session_handle(SESSION_AC).await.unwrap();
        let mut handle_ba = manager_b.session_handle(SESSION_BA).await.unwrap();
        let mut handle_ca = manager_c.session_handle(SESSION_CA).await.unwrap();

        let body_a = Body::new("Hello again from A".as_bytes());
        let body_b = Body::new("Hello again from B".as_bytes());
        let body_c = Body::new("Hello again from C".as_bytes());
        let (peer_a_header_1, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let (peer_b_header_1, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let (peer_c_header_1, _) = peer_c.create_operation(&body_c, LOG_ID).await;

        let operation_a = Operation {
            hash: peer_a_header_1.hash(),
            header: peer_a_header_1.clone(),
            body: Some(body_a.clone()),
        };
        let operation_b = Operation {
            hash: peer_b_header_1.hash(),
            header: peer_b_header_1.clone(),
            body: Some(body_b.clone()),
        };
        let operation_c = Operation {
            hash: peer_c_header_1.hash(),
            header: peer_c_header_1.clone(),
            body: Some(body_c.clone()),
        };

        handle_ab
            .send(ToSync::Payload(operation_a.clone()))
            .await
            .unwrap();
        handle_ac.send(ToSync::Payload(operation_a)).await.unwrap();
        handle_ba.send(ToSync::Payload(operation_b)).await.unwrap();
        handle_ca.send(ToSync::Payload(operation_c)).await.unwrap();

        // Collect all operations each peer receives on the event stream.
        let mut operations_a = vec![];
        let mut operations_b = vec![];
        let mut operations_c = vec![];
        let _ = tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                tokio::select! {
                    Some(event) = event_stream_a.next() => {
                        if let TestTopicSyncEvent::Operation(operation) = event.event() {
                            operations_a.push(*operation.clone());
                        }
                    }
                    Some(event) = event_stream_b.next() => {
                        if let TestTopicSyncEvent::Operation(operation) = event.event() {
                            operations_b.push(*operation.clone());
                        }
                    }
                    Some(event) = event_stream_c.next() => {
                        if let TestTopicSyncEvent::Operation(operation) = event.event() {
                            operations_c.push(*operation.clone());
                        }
                    }
                    else => tokio::time::sleep(Duration::from_millis(5)).await
                }
            }
        })
        .await;

        // All peers received 4 messages, B & C received each other messages via A, and nobody
        // received their own messages.
        assert_eq!(operations_a.len(), 4);
        assert_eq!(operations_b.len(), 4);
        assert_eq!(operations_c.len(), 4);
        assert!(
            operations_a
                .iter()
                .find(|operation| operation.header == peer_a_header_0
                    || operation.header == peer_a_header_1)
                .is_none()
        );
        assert!(
            operations_b
                .iter()
                .find(|operation| operation.header == peer_b_header_0
                    || operation.header == peer_b_header_1)
                .is_none()
        );
        assert!(
            operations_c
                .iter()
                .find(|operation| operation.header == peer_c_header_0
                    || operation.header == peer_c_header_1)
                .is_none()
        );
    }

    #[tokio::test]
    async fn non_blocking_manager_stream() {
        const TOPIC_NAME: &str = "messages";
        const LOG_ID: u64 = 0;
        const SESSION_ID: u64 = 0;

        let topic = TestTopic::new(TOPIC_NAME);

        // Setup Peer A
        let mut peer_a = Peer::new(0);
        let body = Body::new("Hello from Peer A".as_bytes());
        let _ = peer_a.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(peer_a.id(), vec![LOG_ID])]);
        peer_a.insert_topic(&topic, &logs);
        let mut peer_a_manager =
            TopicSyncManager::new(peer_a.topic_map.clone(), peer_a.store.clone());

        // Spawn a task polling peer a's manager stream.
        let mut peer_a_stream = peer_a_manager.subscribe();
        tokio::task::spawn(async move {
            loop {
                peer_a_stream.next().await;
            }
        });

        // Setup Peer B
        let mut peer_b = Peer::new(1);
        let body = Body::new("Hello from Peer B".as_bytes());
        let _ = peer_b.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(peer_b.id(), vec![LOG_ID])]);
        peer_b.insert_topic(&topic, &logs);
        let mut peer_b_manager =
            TopicSyncManager::new(peer_b.topic_map.clone(), peer_b.store.clone());

        // Instantiate sync session for Peer A.
        let config = SyncSessionConfig {
            topic,
            remote: peer_b.id(),
            live_mode: true,
        };

        let peer_a_session = peer_a_manager.session(SESSION_ID, &config).await;

        // Instantiate sync session for Peer B.
        let event_stream = peer_b_manager.subscribe();
        let peer_b_session = peer_b_manager.session(SESSION_ID, &config).await;

        // Get a handle to Peer A sync session.
        let mut peer_a_handle = peer_a_manager.session_handle(SESSION_ID).await.unwrap();

        // Create and send a new live-mode message.
        let (header_1, _) = peer_a.create_operation_no_insert(&body, LOG_ID).await;
        peer_a_handle
            .send(ToSync::Payload(Operation {
                hash: header_1.hash(),
                header: header_1.clone(),
                body: Some(body.clone()),
            }))
            .await
            .unwrap();
        peer_a_handle.send(ToSync::Close).await.unwrap();

        // Actually run the protocol.
        run_protocol(peer_a_session, peer_b_session).await.unwrap();

        // Assert Peer B's events.
        let events = drain_stream(event_stream).await;
        assert_eq!(events.len(), 9);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::SyncStarted(_),
                        ..
                    }
                ),
                1 | 2 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::SyncStatus(_),
                        ..
                    }
                ),
                3 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Operation(_),
                        ..
                    }
                ),
                4 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::SyncFinished(_),
                        ..
                    }
                ),
                5 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::LiveModeStarted,
                        ..
                    }
                ),
                6 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Operation(_),
                        ..
                    }
                ),
                7 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::LiveModeFinished(_),
                        ..
                    }
                ),
                8 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Success,
                        ..
                    }
                ),
                _ => panic!(),
            }
        }
    }
}
