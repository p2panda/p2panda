// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;
use std::future::ready;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use futures::channel::mpsc;
use futures::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::{Body, Extensions, Header, Operation};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::log_sync::{
    LogSyncError, LogSyncEvent, LogSyncMessage, LogSyncMetrics, LogSyncProtocol, Logs, StatusEvent,
};
use crate::traits::Protocol;
use crate::{DEFAULT_BUFFER_CAPACITY, ToSync};

#[derive(Debug)]
pub struct TopicLogSync<T, S, M, L, E> {
    pub store: S,
    pub topic_map: M,
    pub topic: T,
    pub event_tx: broadcast::Sender<TopicLogSyncEvent<E>>,
    pub live_mode_rx: Option<mpsc::Receiver<ToSync<Operation<E>>>>,
    pub buffer_capacity: usize,
    pub _phantom: PhantomData<L>,
}

/// Sync protocol combining TopicHandshake and LogSync protocols into one so that peers can sync
/// logs over a generic T topic. The mapping of T to a set of logs is handled locally by the
/// TopicMap.
///
/// The initiating peer sends their T to the remote and this establishes the topic for the session.
/// After sync is complete peers optionally enter "live-mode" where concurrently received and
/// future messages will be sent directly. As we may receive messages from many sync sessions
/// concurrently, messages forwarded to a sync session in live-mode are de-duplicated in order to
/// avoid flooding the network with redundant data.
impl<T, S, M, L, E> TopicLogSync<T, S, M, L, E>
where
    T: Eq + StdHash + Serialize + for<'a> Deserialize<'a>,
    S: LogStore<L, E> + OperationStore<L, E>,
    M: TopicLogMap<T, L>,
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
{
    /// Returns a new sync protocol instance, configured with a store and `TopicLogMap` implementation
    /// which associates the to-be-synced logs with a given topic.
    pub fn new(
        topic: T,
        store: S,
        topic_map: M,
        live_mode_rx: Option<mpsc::Receiver<ToSync<Operation<E>>>>,
        event_tx: broadcast::Sender<TopicLogSyncEvent<E>>,
    ) -> Self {
        Self::new_with_capacity(
            topic,
            store,
            topic_map,
            live_mode_rx,
            event_tx,
            DEFAULT_BUFFER_CAPACITY,
        )
    }

    pub fn new_with_capacity(
        topic: T,
        store: S,
        topic_map: M,
        live_mode_rx: Option<mpsc::Receiver<ToSync<Operation<E>>>>,
        event_tx: broadcast::Sender<TopicLogSyncEvent<E>>,
        buffer_capacity: usize,
    ) -> Self {
        Self {
            topic,
            topic_map,
            store,
            event_tx,
            live_mode_rx,
            buffer_capacity,
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> Protocol for TopicLogSync<T, S, M, L, E>
where
    T: Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    S: LogStore<L, E> + OperationStore<L, E> + Send + 'static,
    M: TopicLogMap<T, L> + Send + 'static,
    L: LogId + for<'de> Deserialize<'de> + Serialize + Send + 'static,
    E: Extensions + Send + 'static,
{
    type Error = TopicLogSyncError;
    type Message = TopicLogSyncMessage<L, E>;
    type Output = ();

    async fn run(
        self,
        mut sink: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        mut stream: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        // TODO: check there is overlap between the local and remote topic filters and end the
        // session now if not.

        // Get the log ids which are associated with this topic query.
        let logs = self
            .topic_map
            .get(&self.topic)
            .await
            .map_err(|err| TopicLogSyncError::TopicMap(err.to_string()))?;

        // Run the log sync protocol passing in our local topic logs.
        let mut dedup = {
            let (mut log_sync_sink, mut log_sync_stream) = sync_channels(&mut sink, &mut stream);
            let protocol = LogSyncProtocol::new_with_capacity(
                self.store.clone(),
                logs,
                self.event_tx.clone(),
                self.buffer_capacity,
            );
            let result = protocol.run(&mut log_sync_sink, &mut log_sync_stream).await;

            // If the log sync session ended with an error, then send a "failed" event and return
            // here with the error itself.
            match result {
                Ok(dedup) => dedup,
                Err(err) => {
                    self.event_tx
                        .send(TopicLogSyncEvent::Failed {
                            error: err.to_string(),
                        })
                        .map_err(|_| TopicSyncChannelError::EventSend)?;

                    log_sync_sink
                        .close()
                        .await
                        .map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))?;

                    return Err(err.into());
                }
            }
        };

        let Some(mut live_mode_rx) = self.live_mode_rx else {
            sink.close()
                .await
                .map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))?;

            self.event_tx
                .send(TopicLogSyncEvent::Success)
                .map_err(|_| TopicSyncChannelError::EventSend)?;

            return Ok(());
        };

        // Enter live-mode.
        //
        // In live-mode we process messages sent from the remote peer and received locally from a
        // subscription or other concurrent sync sessions. In both cases we should deduplicate
        // messages and also check they are part of our topic sub-set selection before forwarding
        // them on the event stream, or to the remote peer.
        let mut close_sent = false;
        let mut metrics = LiveModeMetrics::default();
        self.event_tx
            .send(TopicLogSyncEvent::LiveModeStarted)
            .map_err(|_| TopicSyncChannelError::EventSend)?;

        let result = loop {
            tokio::select! {
                biased;
                Some(message) = live_mode_rx.next() => {
                    match message {
                        ToSync::Payload(operation) => {
                            if !dedup.insert(operation.hash) {
                                continue;
                            }

                            metrics.bytes_sent +=
                                operation.header.to_bytes().len() as u64 + operation.header.payload_size;
                            metrics.operations_sent += 1;

                            let result = sink
                                .send(TopicLogSyncMessage::Live(
                                    operation.header.clone(),
                                    operation.body.clone(),
                                ))
                                .await
                                .map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")).into());

                            if result.is_err() {
                                break result;
                            };
                        }
                        ToSync::Close => {
                            // We send the close and wait for the remote to close the
                            // connection.
                            let result = sink
                                .send(TopicLogSyncMessage::Close)
                                .await
                                .map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")).into());
                            if result.is_err() {
                                break result;
                            };
                            close_sent = true;
                        }
                    };
                }
                message = stream.next() => {
                    let Some(message) = message else {
                        debug!("remote closed the stream unexpectedly");
                        if close_sent {
                            debug!("remote closed the stream after we sent a close message");
                            break Ok(());
                        }
                        break Err(TopicLogSyncError::UnexpectedStreamClosure);
                    };

                    match message {
                        Ok(message) => {
                            if let TopicLogSyncMessage::Close = message {
                                // We received the remotes close message and should close the
                                // connection ourselves.
                                debug!("received close message from remote");
                                break Ok(());
                            };

                            let TopicLogSyncMessage::Live(header, body) = message else {
                                break Err(TopicLogSyncError::UnexpectedProtocolMessage(
                                    message.to_string(),
                                ));
                            };

                            // @TODO: check that this message is a part of our topic T set.

                            // Insert operation hash into deduplication buffer and if it was
                            // previously present do not forward the operation to the application
                            // layer.
                            if !dedup.insert(header.hash()) {
                                continue;
                            }

                            metrics.bytes_received += header.to_bytes().len() as u64 + header.payload_size;
                            metrics.operations_received += 1;
                            self.event_tx
                                .send(TopicLogSyncEvent::Operation(Box::new(Operation {
                                    hash: header.hash(),
                                    header,
                                    body,
                                })))
                                .map_err(|_| TopicSyncChannelError::EventSend)?;
                        }
                        Err(err) => {
                            if close_sent {
                                debug!("remote closed the stream after we sent a close message");
                                break Ok(());
                            }
                            warn!("error in live-mode: {err:#?}");
                            break Err(TopicLogSyncError::DecodeMessage(format!("{err:?}")));
                        }
                    }
                }
            }
        };

        self.event_tx
            .send(TopicLogSyncEvent::LiveModeFinished(metrics.clone()))
            .map_err(|_| TopicSyncChannelError::EventSend)?;

        sink.close()
            .await
            .map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))?;

        let final_event = match result.as_ref() {
            Ok(_) => TopicLogSyncEvent::Success,
            Err(err) => TopicLogSyncEvent::Failed {
                error: err.to_string(),
            },
        };

        self.event_tx
            .send(final_event)
            .map_err(|_| TopicSyncChannelError::EventSend)?;

        result
    }
}

/// Map raw message sink and stream into log sync protocol specific channels.
#[allow(clippy::complexity)]
pub(crate) fn sync_channels<'a, L, E>(
    sink: &mut (impl Sink<TopicLogSyncMessage<L, E>, Error = impl Debug> + Unpin),
    stream: &mut (impl Stream<Item = Result<TopicLogSyncMessage<L, E>, impl Debug>> + Unpin),
) -> (
    impl Sink<LogSyncMessage<L>, Error = TopicSyncChannelError> + Unpin,
    impl Stream<Item = Result<LogSyncMessage<L>, TopicSyncChannelError>> + Unpin,
) {
    let log_sync_sink = sink
        .sink_map_err(|err| TopicSyncChannelError::MessageSink(format!("{err:?}")))
        .with(|message| {
            ready(Ok::<_, TopicSyncChannelError>(
                TopicLogSyncMessage::<L, E>::Sync(message),
            ))
        });

    let log_sync_stream = stream.by_ref().map(|message| match message {
        Ok(TopicLogSyncMessage::Sync(message)) => Ok(message),
        Ok(TopicLogSyncMessage::Live { .. }) | Ok(TopicLogSyncMessage::Close) => Err(
            TopicSyncChannelError::MessageStream("non-protocol message received".to_string()),
        ),
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

    #[error("no active receivers for broadcast")]
    EventSend,
}

/// Error type occurring in topic log sync protocol.
#[derive(Debug, Error)]
pub enum TopicLogSyncError {
    #[error(transparent)]
    Sync(#[from] LogSyncError),

    #[error("topic map error: {0}")]
    TopicMap(String),

    #[error("unexpected protocol message: {0}")]
    UnexpectedProtocolMessage(String),

    #[error(transparent)]
    Channel(#[from] TopicSyncChannelError),

    #[error("remote unexpectedly closed stream in live-mode")]
    UnexpectedStreamClosure,

    #[error("{0}")]
    DecodeMessage(String),
}

/// Sync metrics emitted in event messages.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LiveModeMetrics {
    pub operations_received: u64,
    pub operations_sent: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopicLogSyncEvent<E> {
    SyncStarted(LogSyncMetrics),
    SyncStatus(LogSyncMetrics),
    SyncFinished(LogSyncMetrics),
    LiveModeStarted,
    LiveModeFinished(LiveModeMetrics),
    Operation(Box<Operation<E>>),
    Success,
    Failed { error: String },
}

impl<E> From<LogSyncEvent<E>> for TopicLogSyncEvent<E> {
    fn from(event: LogSyncEvent<E>) -> Self {
        match event {
            LogSyncEvent::Status(status_event) => match status_event {
                StatusEvent::Started { metrics } => TopicLogSyncEvent::SyncStarted(metrics),
                StatusEvent::Progress { metrics } => TopicLogSyncEvent::SyncStatus(metrics),
                StatusEvent::Completed { metrics } => TopicLogSyncEvent::SyncFinished(metrics),
            },
            LogSyncEvent::Data(operation) => TopicLogSyncEvent::Operation(operation),
        }
    }
}

/// Protocol message types.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
#[allow(clippy::large_enum_variant)]
pub enum TopicLogSyncMessage<L, E> {
    Sync(LogSyncMessage<L>),
    Live(Header<E>, Option<Body>),
    Close,
}

impl<L, E> std::fmt::Display for TopicLogSyncMessage<L, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            TopicLogSyncMessage::Sync(_) => "sync",
            TopicLogSyncMessage::Live(_, _) => "live",
            TopicLogSyncMessage::Close => "close",
        };
        write!(f, "{value}")
    }
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
pub trait TopicLogMap<T, L>: Clone {
    type Error: StdError;

    fn get(&self, topic: &T) -> impl Future<Output = Result<Logs<L>, Self::Error>>;
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use assert_matches::assert_matches;
    use futures::channel::mpsc;
    use futures::{SinkExt, StreamExt};
    use p2panda_core::{Body, Operation};

    use crate::ToSync;
    use crate::log_sync::{LogSyncError, LogSyncMessage};
    use crate::test_utils::{
        Peer, TestTopic, TestTopicSyncEvent, TestTopicSyncMessage, run_protocol, run_protocol_uni,
    };
    use crate::topic_log_sync::{LiveModeMetrics, TopicLogSyncError, TopicLogSyncEvent};
    use crate::traits::Protocol;

    #[tokio::test]
    async fn sync_session_no_operations() {
        let topic = TestTopic::new("messages");
        let mut peer = Peer::new(0);
        peer.insert_topic(&topic, &HashMap::default());

        let (session, mut events_rx, _) = peer.topic_sync_protocol(topic.clone(), false);

        let remote_rx = run_protocol_uni(
            session,
            &[
                TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
                TestTopicSyncMessage::Sync(LogSyncMessage::Done),
            ],
        )
        .await
        .unwrap();

        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStarted(_)
        );
        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncFinished(_)
        );
        assert_matches!(events_rx.recv().await.unwrap(), TestTopicSyncEvent::Success);

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 2);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![]))
                ),
                1 => {
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

        let (session, mut events_rx, _) = peer.topic_sync_protocol(topic.clone(), false);

        let remote_rx = run_protocol_uni(
            session,
            &[
                TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
                TestTopicSyncMessage::Sync(LogSyncMessage::Done),
            ],
        )
        .await
        .unwrap();

        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStarted(_)
        );
        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncFinished(_)
        );
        assert_matches!(events_rx.recv().await.unwrap(), TestTopicSyncEvent::Success);

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 6);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![(
                        peer.id(),
                        vec![(0, 2)]
                    )]))
                ),
                1 => {
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
                2 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_0);
                    assert_eq!(Body::new(&body_inner), body)
                }
                3 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_1);
                    assert_eq!(Body::new(&body_inner), body)
                }
                4 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_2);
                    assert_eq!(Body::new(&body_inner), body)
                }
                5 => {
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

        let (peer_a_session, mut peer_a_events_rx, _) =
            peer_a.topic_sync_protocol(topic.clone(), false);

        let (peer_b_session, mut peer_b_events_rx, _) =
            peer_b.topic_sync_protocol(topic.clone(), false);

        run_protocol(peer_a_session, peer_b_session).await.unwrap();

        // Assert peer a events.
        assert_matches!(
            peer_a_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStarted(_)
        );
        assert_matches!(
            peer_a_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            peer_a_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            peer_a_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncFinished(_)
        );
        assert_matches!(
            peer_a_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::Success
        );

        // Assert peer b events.
        assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStarted(_)
        );
        assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncStatus(_)
        );
        let (header, body_inner) = assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::Operation (operation) => {let Operation {header, body, ..} = *operation; (header, body)}
        );
        assert_eq!(header, header_0);
        assert_eq!(body_inner.unwrap(), body);
        let (header, body_inner) = assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::Operation (operation) => {let Operation {header, body, ..} = *operation; (header, body)}
        );
        assert_eq!(header, header_1);
        assert_eq!(body_inner.unwrap(), body);
        let (header, body_inner) = assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::Operation (operation) => {let Operation {header, body, ..} = *operation; (header, body)}
        );
        assert_eq!(header, header_2);
        assert_eq!(body_inner.unwrap(), body);
        assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::SyncFinished(_)
        );
        assert_matches!(
            peer_b_events_rx.recv().await.unwrap(),
            TestTopicSyncEvent::Success
        );
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

        let (protocol, mut events_rx, mut live_mode_tx) =
            peer_a.topic_sync_protocol(topic.clone(), true);

        live_mode_tx
            .send(ToSync::Payload(Operation {
                hash: header_2.hash(),
                header: header_2.clone(),
                body: Some(body.clone()),
            }))
            .await
            .unwrap();
        live_mode_tx.send(ToSync::Close).await.unwrap();

        let total_bytes = header_bytes_0.len() + body.to_bytes().len();
        let remote_rx = run_protocol_uni(
            protocol,
            &[
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
                TestTopicSyncMessage::Live(header_1.clone(), Some(body.clone())),
                TestTopicSyncMessage::Close,
            ],
        )
        .await
        .unwrap();

        for index in 0..=8 {
            let event = events_rx.recv().await.unwrap();
            match index {
                0 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncStarted(_));
                }
                1 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncStatus(_));
                }
                2 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncStatus(_));
                }
                3 => {
                    assert_matches!(event, TestTopicSyncEvent::Operation(_));
                }
                4 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncFinished(_));
                }
                5 => {
                    assert_matches!(event, TestTopicSyncEvent::LiveModeStarted);
                }
                6 => {
                    assert_matches!(event, TestTopicSyncEvent::Operation(_));
                }
                7 => {
                    let metrics = assert_matches!(event, TestTopicSyncEvent::LiveModeFinished(metrics) => metrics);
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
                8 => {
                    assert_matches!(event, TestTopicSyncEvent::Success)
                }
                _ => panic!(),
            };
        }

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 4);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Have(_))),
                1 => {
                    assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Done))
                }
                2 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Live(
                    header,
                    Some(body)
                ) => (header, body));
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner, body);
                }
                3 => {
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

        let (protocol, mut events_rx, mut live_mode_tx) =
            peer_a.topic_sync_protocol(topic.clone(), true);

        live_mode_tx
            .send(ToSync::Payload(Operation {
                hash: header_2.hash(),
                header: header_2.clone(),
                body: Some(body.clone()),
            }))
            .await
            .unwrap();

        // Sending subscription message twice.
        live_mode_tx
            .send(ToSync::Payload(Operation {
                hash: header_2.hash(),
                header: header_2.clone(),
                body: Some(body.clone()),
            }))
            .await
            .unwrap();

        live_mode_tx.send(ToSync::Close).await.unwrap();

        let total_bytes = header_bytes_0.len() + body.to_bytes().len();
        let remote_rx = run_protocol_uni(
            protocol,
            &[
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
                TestTopicSyncMessage::Live(header_1.clone(), Some(body.clone())),
                // Duplicate of message sent during sync.
                TestTopicSyncMessage::Live(header_0.clone(), Some(body.clone())),
                // Duplicate of message sent earlier in live mode.
                TestTopicSyncMessage::Live(header_1.clone(), Some(body.clone())),
                TestTopicSyncMessage::Close,
            ],
        )
        .await
        .unwrap();

        for index in 0..=8 {
            let event = events_rx.recv().await.unwrap();
            match index {
                0 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncStarted(_));
                }
                1 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncStatus(_));
                }
                2 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncStatus(_));
                }
                3 => {
                    assert_matches!(event, TestTopicSyncEvent::Operation(_));
                }
                4 => {
                    assert_matches!(event, TestTopicSyncEvent::SyncFinished(_));
                }
                5 => {
                    assert_matches!(event, TestTopicSyncEvent::LiveModeStarted);
                }
                6 => {
                    assert_matches!(event, TestTopicSyncEvent::Operation(_));
                }
                7 => {
                    let metrics = assert_matches!(event, TestTopicSyncEvent::LiveModeFinished(metrics) => metrics);
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
                8 => {
                    assert_matches!(event, TestTopicSyncEvent::Success)
                }
                _ => panic!(),
            };
        }

        let messages = remote_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 4);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Have(_))),
                1 => {
                    assert_matches!(message, TestTopicSyncMessage::Sync(LogSyncMessage::Done))
                }
                2 => {
                    let (header, body_inner) = assert_matches!(message, TestTopicSyncMessage::Live(header, Some(body)) => (header, body));
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner, body);
                }
                3 => {
                    assert_matches!(message, TestTopicSyncMessage::Close)
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn unexpected_stream_closure_sync() {
        let topic = TestTopic::new("messages");
        let mut peer = Peer::new(0);
        peer.insert_topic(&topic, &Default::default());

        let (session, mut events_rx, _live_tx) = peer.topic_sync_protocol(topic.clone(), true);

        let messages = [TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![]))];

        let (mut local_message_tx, _remote_message_rx) = mpsc::channel(128);
        let (mut remote_message_tx, local_message_rx) = mpsc::channel(128);
        let mut local_message_rx = local_message_rx.map(|message| Ok::<_, ()>(message));

        for message in messages {
            remote_message_tx.send(message.to_owned()).await.unwrap();
        }

        let handle = tokio::spawn(async move {
            session
                .run(&mut local_message_tx, &mut local_message_rx)
                .await
                .expect_err("expected unexpected stream closure")
        });

        drop(remote_message_tx);

        let err = handle.await.unwrap();
        assert_matches!(
            err,
            TopicLogSyncError::Sync(LogSyncError::UnexpectedStreamClosure)
        );

        while let Ok(event) = events_rx.recv().await {
            if let TopicLogSyncEvent::Failed { error } = event {
                assert_eq!(
                    error,
                    "remote unexpectedly closed stream during initial sync".to_string()
                );
                break;
            }
        }
    }

    #[tokio::test]
    async fn unexpected_stream_closure_live_mode() {
        let topic = TestTopic::new("messages");
        let mut peer = Peer::new(0);
        peer.insert_topic(&topic, &Default::default());

        let (session, mut events_rx, _live_tx) = peer.topic_sync_protocol(topic.clone(), true);

        let messages = [
            TestTopicSyncMessage::Sync(LogSyncMessage::Have(vec![])),
            TestTopicSyncMessage::Sync(LogSyncMessage::Done),
        ];

        let (mut local_message_tx, _remote_message_rx) = mpsc::channel(128);
        let (mut remote_message_tx, local_message_rx) = mpsc::channel(128);
        let mut local_message_rx = local_message_rx.map(|message| Ok::<_, ()>(message));

        for message in messages {
            remote_message_tx.send(message.to_owned()).await.unwrap();
        }

        let handle = tokio::spawn(async move {
            session
                .run(&mut local_message_tx, &mut local_message_rx)
                .await
                .expect_err("expected unexpected stream closure")
        });

        drop(remote_message_tx);

        let err = handle.await.unwrap();
        assert_matches!(err, TopicLogSyncError::UnexpectedStreamClosure);

        while let Ok(event) = events_rx.recv().await {
            if let TopicLogSyncEvent::Failed { error } = event {
                assert_eq!(
                    error,
                    "remote unexpectedly closed stream in live-mode".to_string()
                );
                break;
            }
        }
    }
}
