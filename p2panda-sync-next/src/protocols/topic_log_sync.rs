// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::future::ready;
use std::marker::PhantomData;

use futures::channel::mpsc;
use futures::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::{Body, Extensions, Header};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::DEFAULT_BUFFER_CAPACITY;
use crate::log_sync::{LogSyncError, LogSyncEvent, LogSyncMessage, LogSyncProtocol, Logs};
use crate::topic_handshake::{
    TopicHandshakeAcceptor, TopicHandshakeError, TopicHandshakeEvent, TopicHandshakeInitiator,
    TopicHandshakeMessage,
};
use crate::traits::{Protocol, TopicQuery};

pub struct TopicLogSync<T, S, M, L, E> {
    pub store: S,
    pub topic_map: M,
    pub role: Role<T>,
    pub event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
    pub live_mode_rx: Option<mpsc::Receiver<LiveModeMessage<E>>>,
    pub buffer_capacity: usize,
    pub _phantom: PhantomData<L>,
}

/// Sync protocol combining TopicHandshake and LogSync protocols into one so that peers can sync
/// logs over a generic T topic. The mapping of T to a set of logs is handled locally by the
/// TopicMap. The initiating peer sends their T to the remote and this establishes the topic for
/// the session. After sync is complete peers optionally enter "live-mode" where concurrently
/// received and future messages will be sent directly. As we may receive messages from many sync
/// sessions concurrently, messages forwarded to a sync session in live-mode are de-duplicated in
/// order to avoid flooding the network with redundant data.
impl<T, S, M, L, E> TopicLogSync<T, S, M, L, E>
where
    // @TODO: We can require TopicId here as well so that the receiving peer can check that the
    // topic id is one that they are subscribed to. We don't yet have access to the required
    // subscriptions though.
    T: TopicQuery,
    S: LogStore<L, E> + OperationStore<L, E> + Clone,
    M: TopicLogMap<T, L> + Clone,
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
{
    /// Returns a new sync protocol instance, configured with a store and `TopicLogMap` implementation
    /// which associates the to-be-synced logs with a given topic.
    pub fn new(
        store: S,
        topic_map: M,
        role: Role<T>,
        live_mode_rx: Option<mpsc::Receiver<LiveModeMessage<E>>>,
        event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
    ) -> Self {
        Self::new_with_capacity(
            store,
            topic_map,
            role,
            live_mode_rx,
            event_tx,
            DEFAULT_BUFFER_CAPACITY,
        )
    }

    pub fn new_with_capacity(
        store: S,
        topic_map: M,
        role: Role<T>,
        live_mode_rx: Option<mpsc::Receiver<LiveModeMessage<E>>>,
        event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
        buffer_capacity: usize,
    ) -> Self {
        Self {
            topic_map,
            store,
            role,
            event_tx,
            live_mode_rx,
            buffer_capacity,
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> Protocol for TopicLogSync<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E> + OperationStore<L, E> + Clone,
    M: TopicLogMap<T, L> + Clone,
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
{
    type Error = TopicLogSyncError<T, S, M, L, E>;
    type Event = TopicLogSyncEvent<T, E>;
    type Message = TopicLogSyncMessage<T, L, E>;
    type Output = ();

    async fn run(
        mut self,
        mut sink: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        mut stream: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        // Run topic handshake protocol to agree on the topic for this sync session.
        let topic = {
            let (mut topic_sink, mut topic_stream) = topic_channels(&mut sink, &mut stream);

            match &self.role {
                Role::Initiate(topic) => {
                    let protocol =
                        TopicHandshakeInitiator::new(topic.clone(), self.event_tx.clone());
                    protocol.run(&mut topic_sink, &mut topic_stream).await?;
                    topic.clone()
                }
                Role::Accept => {
                    let protocol = TopicHandshakeAcceptor::new(self.event_tx.clone());
                    protocol.run(&mut topic_sink, &mut topic_stream).await?
                }
            }
        };

        // @TODO: check there is overlap between the local and remote topic filters and end the
        // session now if not.

        // Get the log ids which are associated with this topic query.
        let logs = self
            .topic_map
            .get(&topic)
            .await
            .map_err(TopicLogSyncError::<T, S, M, L, E>::TopicMap)?;

        // Run the log sync protocol passing in our local topic logs.
        let mut dedup = {
            let (mut log_sync_sink, mut log_sync_stream) = sync_channels(&mut sink, &mut stream);
            let protocol = LogSyncProtocol::new_with_capacity(
                self.store.clone(),
                logs,
                self.event_tx.clone(),
                self.buffer_capacity,
            );
            protocol
                .run(&mut log_sync_sink, &mut log_sync_stream)
                .await?
        };

        // Enter live-mode.
        //
        // In live-mode we process messages sent from the remote peer and received locally from a
        // subscription or other concurrent sync sessions. In both cases we should deduplicate
        // messages and also check they are part of our topic sub-set selection before forwarding
        // them on the event stream, or to the remote peer.
        let mut metrics = LiveModeMetrics::default();
        if let Some(mut live_mode_rx) = self.live_mode_rx {
            loop {
                tokio::select! {
                    biased;
                    Some(message) = stream.next() => {
                        let message = message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;
                        if let TopicLogSyncMessage::Close = message {
                            self.event_tx.send(TopicLogSyncEvent::Close{metrics}).await.map_err(TopicSyncChannelError::EventSend)?;
                            return Ok(());
                        };

                        let TopicLogSyncMessage::Live{header, body} = message else {
                            return Err(TopicLogSyncError::UnexpectedProtocolMessage(Box::new(message)));
                        };
                        // @TODO: check that this message is a part of our topic T set.

                        // Insert operation hash into deduplication buffer and if it was
                        // previously present do not forward the operation to the application layer.
                        if !dedup.insert(header.hash()) {
                            continue;
                        }

                        metrics.bytes_received += header.to_bytes().len()  as u64 + header.payload_size;
                        metrics.operations_received += 1;
                        self.event_tx.send(TopicLogSyncEvent::Live { header: Box::new(header), body }).await.map_err(TopicSyncChannelError::EventSend)?;
                    }
                    Some(message) = live_mode_rx.next() => {
                        match message {
                            LiveModeMessage::Operation { header, body } => {
                                if !dedup.insert(header.hash()) {
                                    continue;
                                }

                                metrics.bytes_sent += header.to_bytes().len()  as u64 + header.payload_size;
                                metrics.operations_sent += 1;
                                sink.send(TopicLogSyncMessage::Live { header: *header.clone(), body: body.clone() })
                                    .await
                                    .map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))?;
                            }
                            LiveModeMessage::Close => {
                                self.event_tx.send(TopicLogSyncEvent::Close{metrics}).await.map_err(TopicSyncChannelError::EventSend)?;
                                sink.send(TopicLogSyncMessage::Close).await.map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))?;
                                return Ok(())
                            },
                        };
                    }
                }
            }
        }

        Ok(())
    }
}

/// Map raw message sink and stream into topic handshake protocol specific channels.
#[allow(clippy::complexity)]
pub(crate) fn topic_channels<'a, T, L, E>(
    sink: &mut (impl Sink<TopicLogSyncMessage<T, L, E>, Error = impl Debug> + Unpin),
    stream: &mut (impl Stream<Item = Result<TopicLogSyncMessage<T, L, E>, impl Debug>> + Unpin),
) -> (
    impl Sink<TopicHandshakeMessage<T>, Error = TopicSyncChannelError> + Unpin,
    impl Stream<Item = Result<TopicHandshakeMessage<T>, TopicSyncChannelError>> + Unpin,
) {
    let topic_sink = sink
        .sink_map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))
        .with(|message| {
            ready(Ok::<_, TopicSyncChannelError>(
                TopicLogSyncMessage::<T, L, E>::Handshake(message),
            ))
        });
    let topic_stream = stream.by_ref().map(|message| {
        let message =
            message.map_err(|err| TopicSyncChannelError::MessageStream(format!("{err:?}")))?;
        match message {
            TopicLogSyncMessage::Handshake(message) => Ok(message),
            TopicLogSyncMessage::Sync(_)
            | TopicLogSyncMessage::Live { .. }
            | TopicLogSyncMessage::Close => Err(TopicSyncChannelError::MessageStream(
                "non-protocol message received".to_string(),
            )),
        }
    });
    (topic_sink, topic_stream)
}

/// Map raw message sink and stream into log sync protocol specific channels.
#[allow(clippy::complexity)]
pub(crate) fn sync_channels<'a, T, L, E>(
    sink: &mut (impl Sink<TopicLogSyncMessage<T, L, E>, Error = impl Debug> + Unpin),
    stream: &mut (impl Stream<Item = Result<TopicLogSyncMessage<T, L, E>, impl Debug>> + Unpin),
) -> (
    impl Sink<LogSyncMessage<L>, Error = TopicSyncChannelError> + Unpin,
    impl Stream<Item = Result<LogSyncMessage<L>, TopicSyncChannelError>> + Unpin,
) {
    let log_sync_sink = sink
        .sink_map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))
        .with(|message| {
            ready(Ok::<_, TopicSyncChannelError>(
                TopicLogSyncMessage::<T, L, E>::Sync(message),
            ))
        });
    let log_sync_stream = stream.by_ref().map(|message| match message {
        Ok(TopicLogSyncMessage::Sync(message)) => Ok(message),
        Ok(TopicLogSyncMessage::Handshake(_))
        | Ok(TopicLogSyncMessage::Live { .. })
        | Ok(TopicLogSyncMessage::Close) => Err(TopicSyncChannelError::MessageStream(
            "non-protocol message received".to_string(),
        )),
        Err(err) => Err(TopicSyncChannelError::MessageStream(format!("{err:?}"))),
    });

    (log_sync_sink, log_sync_stream)
}

/// Error type occurring in topic log sync protocol.
#[derive(Debug, Error)]
pub enum TopicSyncChannelError {
    #[error("error sending on message sink: {0}")]
    MessageSink(String),

    #[error("error receiving from message stream: {0}")]
    MessageStream(String),

    #[error(transparent)]
    EventSend(#[from] mpsc::SendError),
}

/// Error type occurring in topic log sync protocol.
#[derive(Debug, Error)]
pub enum TopicLogSyncError<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E> + OperationStore<L, E>,
    M: TopicLogMap<T, L>,
{
    #[error(transparent)]
    TopicHandshake(#[from] TopicHandshakeError<T>),

    #[error(transparent)]
    Sync(#[from] LogSyncError<L, E, S>),

    #[error(transparent)]
    TopicMap(M::Error),

    #[error("unexpected protocol message: {0:?}")]
    UnexpectedProtocolMessage(Box<TopicLogSyncMessage<T, L, E>>),

    #[error(transparent)]
    Channel(#[from] TopicSyncChannelError),
}

/// Sync metrics emitted in event messages.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LiveModeMetrics {
    pub operations_received: u64,
    pub operations_sent: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
}

/// Topic log sync live-mode message types.
#[derive(Clone, Debug)]
pub enum LiveModeMessage<E> {
    /// Operation received from a subscription or a concurrent sync session (via the manager).
    Operation {
        header: Box<Header<E>>,
        body: Option<Body>,
    },
    /// Gracefully close the session.
    Close,
}

/// "initiator" and an "acceptor" roles representing both parties in a log sync protocol session.
///
/// The only difference is that the initiator determines the T topic of the sync session.
#[derive(Clone, Debug)]
pub enum Role<T> {
    Initiate(T),
    Accept,
}

/// Events emitted by a sync session.
#[derive(Debug, Clone, PartialEq)]
pub enum TopicLogSyncEvent<T, E> {
    Handshake(TopicHandshakeEvent<T>),
    Sync(LogSyncEvent<E>),
    Live {
        header: Box<Header<E>>,
        body: Option<Body>,
    },
    Close {
        metrics: LiveModeMetrics,
    },
}

/// Conversion trait required by TopicHandshakeProtocol.
impl<T, E> From<TopicHandshakeEvent<T>> for TopicLogSyncEvent<T, E> {
    fn from(value: TopicHandshakeEvent<T>) -> Self {
        TopicLogSyncEvent::Handshake(value)
    }
}

/// Conversion trait required by LogSyncProtocol.
impl<T, E> From<LogSyncEvent<E>> for TopicLogSyncEvent<T, E> {
    fn from(value: LogSyncEvent<E>) -> Self {
        TopicLogSyncEvent::Sync(value)
    }
}

/// Protocol message types.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
#[allow(clippy::large_enum_variant)]
pub enum TopicLogSyncMessage<T, L, E> {
    Handshake(TopicHandshakeMessage<T>),
    Sync(LogSyncMessage<L>),
    Live {
        header: Header<E>,
        body: Option<Body>,
    },
    Close,
}

/// Maps a `TopicQuery` to the related logs being sent over the wire during sync.
///
/// Each `SyncProtocol` implementation defines the type of data it is expecting to sync and how
/// the scope for a particular session should be identified. `LogSyncProtocol` maps a generic
/// `TopicQuery` to a set of logs; users provide an implementation of the `TopicLogMap` trait in
/// order to define how this mapping occurs.
///
/// Since `TopicLogMap` is generic we can use the same mapping across different sync implementations
/// for the same data type when necessary.
///
/// ## Designing `TopicLogMap` for applications
///
/// Considering an example chat application which is based on append-only log data types, we
/// probably want to organise messages from an author for a certain chat group into one log each.
/// Like this, a chat group can be expressed as a collection of one to potentially many logs (one
/// per member of the group):
///
/// ```text
/// All authors: A, B and C
/// All chat groups: 1 and 2
///
/// "Chat group 1 with members A and B"
/// - Log A1
/// - Log B1
///
/// "Chat group 2 with members A, B and C"
/// - Log A2
/// - Log B2
/// - Log C2
/// ```
///
/// If we implement `TopicQuery` to express that we're interested in syncing over a specific chat
/// group, for example "Chat Group 2" we would implement `TopicLogMap` to give us all append-only
/// logs of all members inside this group, that is the entries inside logs `A2`, `B2` and `C2`.
pub trait TopicLogMap<T, L>
where
    T: TopicQuery,
{
    type Error: Debug;

    fn get(&self, topic: &T) -> impl Future<Output = Result<Logs<L>, Self::Error>>;
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use assert_matches::assert_matches;
    use futures::{SinkExt, StreamExt};
    use p2panda_core::{Body, Operation};

    use crate::log_sync::{LogSyncEvent, LogSyncMessage, StatusEvent};
    use crate::test_utils::{
        Peer, TestTopic, TestTopicSyncEvent, TestTopicSyncMessage, run_protocol, run_protocol_uni,
    };
    use crate::topic_handshake::{TopicHandshakeEvent, TopicHandshakeMessage};
    use crate::topic_log_sync::{LiveModeMessage, LiveModeMetrics, Role};

    #[tokio::test]
    async fn sync_session_no_operations() {
        let topic = TestTopic::new("messages");
        let mut peer = Peer::new(0);
        peer.insert_topic(&topic, &HashMap::default());

        let (session, events_rx, _) =
            peer.topic_sync_protocol(Role::Initiate(topic.clone()), false);

        let remote_rx = run_protocol_uni(
            session,
            &[
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
                TestTopicSyncMessage::Sync(LogSyncMessage::Done),
            ],
        )
        .await
        .unwrap();

        let events = events_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 6);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => {
                    assert_eq!(
                        event,
                        TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Initiate(topic.clone()))
                    )
                }
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(topic.clone()))
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ))
                    )
                }
                _ => panic!(),
            };
        }

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 4);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Topic(topic.clone()))
                ),
                1 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done)
                ),
                2 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![]))
                ),
                3 => {
                    assert_eq!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Done));
                    break;
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn sync_operations_accept() {
        let log_id = 0;
        let topic = TestTopic::new("messages");
        let mut peer = Peer::new(0);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, header_bytes_0) = peer.create_operation(&body, log_id).await;
        let (header_1, header_bytes_1) = peer.create_operation(&body, log_id).await;
        let (header_2, header_bytes_2) = peer.create_operation(&body, log_id).await;

        let logs = HashMap::from([(peer.id(), vec![log_id])]);
        peer.insert_topic(&topic, &logs);

        let (session, events_rx, _) = peer.topic_sync_protocol(Role::Accept, false);

        let remote_rx = run_protocol_uni(
            session,
            &[
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Topic(topic.clone())),
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
                TestTopicSyncMessage::Sync(LogSyncMessage::Done),
            ],
        )
        .await
        .unwrap();

        let events = events_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 7);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => {
                    assert_eq!(
                        event,
                        TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                    )
                }
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(
                        topic.clone()
                    ))
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(_))
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                6 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ))
                    )
                }
                _ => panic!(),
            };
        }

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 7);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                ),
                1 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![(
                        peer.id(),
                        vec![(0, 2)]
                    )]))
                ),
                2 => {
                    let expected_bytes = header_0.payload_size
                        + header_bytes_0.len() as u64
                        + header_1.payload_size
                        + header_bytes_1.len() as u64
                        + header_2.payload_size
                        + header_bytes_2.len() as u64;

                    assert_eq!(
                        message,
                        TestTopicSyncMessage::Sync(LogSyncMessage::PreSync {
                            total_operations: 3,
                            total_bytes: expected_bytes
                        })
                    )
                }
                3 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_0);
                    assert_eq!(Body::new(&body_inner), body)
                }
                4 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_1);
                    assert_eq!(Body::new(&body_inner), body)
                }
                5 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_2);
                    assert_eq!(Body::new(&body_inner), body)
                }
                6 => {
                    assert_eq!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Done));
                    break;
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn topic_log_sync_full_duplex() {
        let topic = TestTopic::new("messages");
        let log_id = 0;

        let mut peer_a = Peer::new(0);
        let mut peer_b = Peer::new(1);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, _) = peer_a.create_operation(&body, 0).await;
        let (header_1, _) = peer_a.create_operation(&body, 0).await;
        let (header_2, _) = peer_a.create_operation(&body, 0).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        let (peer_a_session, peer_a_events_rx, _) =
            peer_a.topic_sync_protocol(Role::Initiate(topic.clone()), false);

        let (peer_b_session, peer_b_events_rx, _) = peer_b.topic_sync_protocol(Role::Accept, false);

        run_protocol(peer_a_session, peer_b_session).await.unwrap();

        let events = peer_a_events_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 6);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Initiate(
                        sent_topic,
                    ))
                    if sent_topic == topic
                ),
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(topic.clone()))
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                }
                _ => panic!(),
            };
        }

        let events = peer_b_events_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 10);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                ),
                1 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(received_topic))
                    if received_topic == topic
                ),
                2 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(topic.clone()))
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }))
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    );
                }
                6 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_0);
                    assert_eq!(body_inner.unwrap(), body);
                }
                7 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_1);
                    assert_eq!(body_inner.unwrap(), body);
                }
                8 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner.unwrap(), body);
                }
                9 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn live_mode() {
        let log_id = 0;
        let topic = TestTopic::new("messages");
        let mut peer_a = Peer::new(0);
        let mut peer_b = Peer::new(1);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (_, header_bytes_0) = peer_b.create_operation(&body, log_id).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        let logs = HashMap::default();
        peer_a.insert_topic(&topic, &logs);

        let (header_1, _) = peer_b.create_operation_no_insert(&body, log_id).await;
        let expected_live_mode_bytes_received =
            header_1.payload_size + header_1.to_bytes().len() as u64;
        let (header_2, _) = peer_a.create_operation_no_insert(&body, log_id).await;
        let expected_live_mode_bytes_sent =
            header_2.payload_size + header_2.to_bytes().len() as u64;

        let (protocol, events_rx, mut live_mode_tx) =
            peer_a.topic_sync_protocol(Role::Accept, true);

        live_mode_tx
            .send(LiveModeMessage::Operation {
                header: Box::new(header_2.clone()),
                body: Some(body.clone()),
            })
            .await
            .unwrap();

        live_mode_tx.send(LiveModeMessage::Close).await.unwrap();

        let total_bytes = header_bytes_0.len() + body.to_bytes().len();
        let remote_rx = run_protocol_uni(
            protocol,
            &[
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Topic(topic.clone())),
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
                TestTopicSyncMessage::Sync(LogSyncMessage::PreSync {
                    total_operations: 1,
                    total_bytes: total_bytes as u64,
                }),
                TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header_bytes_0,
                    Some(body.to_bytes()),
                )),
                TestTopicSyncMessage::Sync(LogSyncMessage::Done),
                TestTopicSyncMessage::Live {
                    header: header_1.clone(),
                    body: Some(body.clone()),
                },
            ],
        )
        .await
        .unwrap();

        let events = events_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 10);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => {
                    assert_eq!(
                        event,
                        TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                    )
                }
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(
                        topic.clone()
                    ))
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(_))
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                6 => {
                    assert_matches!(event, TestTopicSyncEvent::Sync(LogSyncEvent::Data(..)))
                }
                7 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                }
                8 => {
                    assert_matches!(event, TestTopicSyncEvent::Live { .. });
                }
                9 => {
                    let metrics =
                        assert_matches!(event, TestTopicSyncEvent::Close { metrics } => metrics);
                    let LiveModeMetrics {
                        operations_received,
                        operations_sent,
                        bytes_received,
                        bytes_sent,
                    } = metrics;
                    assert_eq!(operations_received, 1);
                    assert_eq!(operations_sent, 1);
                    assert_eq!(bytes_received, expected_live_mode_bytes_received);
                    assert_eq!(bytes_sent, expected_live_mode_bytes_sent);
                }
                _ => panic!(),
            };
        }

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 5);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                ),
                1 => assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Have(_))),
                2 => {
                    assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Done))
                }
                3 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Live{
                    header,
                    body: Some(body),
                } => (header, body));
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner, body);
                }
                4 => {
                    assert_matches!(message, TestTopicSyncMessage::Close)
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn dedup_live_mode_messages() {
        let log_id = 0;
        let topic = TestTopic::new("messages");
        let mut peer_a = Peer::new(0);
        let mut peer_b = Peer::new(1);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, header_bytes_0) = peer_b.create_operation(&body, log_id).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        let logs = HashMap::default();
        peer_a.insert_topic(&topic, &logs);

        let (header_1, _) = peer_b.create_operation_no_insert(&body, log_id).await;
        let expected_live_mode_bytes_received =
            header_1.payload_size + header_1.to_bytes().len() as u64;
        let (header_2, _) = peer_a.create_operation_no_insert(&body, log_id).await;
        let expected_live_mode_bytes_sent =
            header_2.payload_size + header_2.to_bytes().len() as u64;

        let (protocol, events_rx, mut live_mode_tx) =
            peer_a.topic_sync_protocol(Role::Accept, true);

        live_mode_tx
            .send(LiveModeMessage::Operation {
                header: Box::new(header_2.clone()),
                body: Some(body.clone()),
            })
            .await
            .unwrap();

        // Sending subscription message twice.
        live_mode_tx
            .send(LiveModeMessage::Operation {
                header: Box::new(header_2.clone()),
                body: Some(body.clone()),
            })
            .await
            .unwrap();

        live_mode_tx.send(LiveModeMessage::Close).await.unwrap();

        let total_bytes = header_bytes_0.len() + body.to_bytes().len();
        let remote_rx = run_protocol_uni(
            protocol,
            &[
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Topic(topic.clone())),
                TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
                TestTopicSyncMessage::Sync(LogSyncMessage::PreSync {
                    total_operations: 1,
                    total_bytes: total_bytes as u64,
                }),
                TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header_bytes_0,
                    Some(body.to_bytes()),
                )),
                TestTopicSyncMessage::Sync(LogSyncMessage::Done),
                TestTopicSyncMessage::Live {
                    header: header_1.clone(),
                    body: Some(body.clone()),
                },
                // Duplicate of message sent during sync.
                TestTopicSyncMessage::Live {
                    header: header_0.clone(),
                    body: Some(body.clone()),
                },
                // Duplicate of message sent earlier in live mode.
                TestTopicSyncMessage::Live {
                    header: header_1.clone(),
                    body: Some(body.clone()),
                },
            ],
        )
        .await
        .unwrap();

        let events = events_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 10);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => {
                    assert_eq!(
                        event,
                        TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                    )
                }
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(
                        topic.clone()
                    ))
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done(_))
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                6 => {
                    assert_matches!(event, TestTopicSyncEvent::Sync(LogSyncEvent::Data(..)))
                }
                7 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                }
                8 => {
                    assert_matches!(event, TestTopicSyncEvent::Live { .. });
                }
                9 => {
                    let metrics =
                        assert_matches!(event, TestTopicSyncEvent::Close { metrics } => metrics);
                    let LiveModeMetrics {
                        operations_received,
                        operations_sent,
                        bytes_received,
                        bytes_sent,
                    } = metrics;
                    assert_eq!(operations_received, 1);
                    assert_eq!(operations_sent, 1);
                    assert_eq!(bytes_received, expected_live_mode_bytes_received);
                    assert_eq!(bytes_sent, expected_live_mode_bytes_sent);
                }
                _ => panic!(),
            };
        }

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 5);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Handshake(TopicHandshakeMessage::Done),
                ),
                1 => assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Have(_))),
                2 => {
                    assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Done))
                }
                3 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Live{
                header,
                body: Some(body),
            } => (header, body));
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner, body);
                }
                4 => {
                    assert_matches!(message, TestTopicSyncMessage::Close)
                }
                _ => panic!(),
            };
        }
    }
}
