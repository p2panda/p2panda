use std::fmt::Debug;
use std::future::ready;
use std::marker::PhantomData;

use crate::cbor::{CborCodecError, into_cbor_sink, into_cbor_stream};
use crate::log_sync::{LogSyncError, LogSyncEvent, LogSyncMessage, LogSyncProtocol, Logs};
use crate::topic_handshake::{
    TopicHandshakeAcceptor, TopicHandshakeError, TopicHandshakeEvent, TopicHandshakeInitiator,
    TopicHandshakeMessage,
};
use crate::traits::{Protocol, SyncProtocol, TopicQuery};
use futures::channel::mpsc;
use futures::{AsyncRead, AsyncWrite, Sink, SinkExt, Stream, StreamExt};
use p2panda_core::{Body, Extensions, Header};
use p2panda_store::{LogId, LogStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;

pub struct TopicLogSync<T, S, M, L, E> {
    pub store: S,
    pub topic_map: M,
    pub role: Role<T>,
    pub event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
    pub live_mode_rx: Option<broadcast::Receiver<LiveModeMessage<E>>>,
    pub _phantom: PhantomData<(T, L, E)>,
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
    S: LogStore<L, E> + Clone,
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
        live_mode_rx: Option<broadcast::Receiver<LiveModeMessage<E>>>,
        event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
    ) -> Self {
        Self {
            topic_map,
            store,
            role,
            event_tx,
            live_mode_rx,
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> SyncProtocol for TopicLogSync<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E> + Clone,
    M: TopicLogMap<T, L> + Clone,
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
{
    type Error = TopicLogSyncError<T, S, M, L, E>;
    type Event = TopicLogSyncEvent<T, E>;
    type Output = ();

    async fn run(
        self,
        tx: &mut (impl AsyncWrite + Unpin),
        rx: &mut (impl AsyncRead + Unpin),
    ) -> Result<(), TopicLogSyncError<T, S, M, L, E>> {
        // Convert generic read-write channels into framed sink and stream of cbor encoded protocol messages.
        let mut sink = into_cbor_sink::<TopicLogSyncMessage<T, L, E>>(tx);
        let mut stream = into_cbor_stream::<TopicLogSyncMessage<T, L, E>>(rx);

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
        {
            let (mut log_sync_sink, mut log_sync_stream) = sync_channels(&mut sink, &mut stream);
            let protocol = LogSyncProtocol::new(self.store.clone(), logs, self.event_tx.clone());
            protocol
                .run(&mut log_sync_sink, &mut log_sync_stream)
                .await?;
        }

        // Enter live mode.
        if let Some(mut live_mode_rx) = self.live_mode_rx {
            if let Ok(message) = live_mode_rx.recv().await {
                match message {
                    LiveModeMessage::FromSub { header, body } => {
                        sink.send(TopicLogSyncMessage::Live(header, body)).await?;
                    }
                    LiveModeMessage::FromSync { header, body } => {
                        // @TODO: deduplicate messages.
                        // @TODO: check that this message is a part of our topic T set.
                        sink.send(TopicLogSyncMessage::Live(header, body)).await?;
                    }
                    LiveModeMessage::Close => return Ok(()),
                }
            }
        }

        Ok(())
    }
}

/// Map raw message sink and stream into topic handshake protocol specific channels.
#[allow(clippy::complexity)]
pub(crate) fn topic_channels<'a, T, L, E>(
    sink: &mut (impl Sink<TopicLogSyncMessage<T, L, E>, Error = CborCodecError> + Unpin),
    stream: &mut (impl Stream<Item = Result<TopicLogSyncMessage<T, L, E>, CborCodecError>> + Unpin),
) -> (
    impl Sink<TopicHandshakeMessage<T>, Error = TopicHandshakeError<T>> + Unpin,
    impl Stream<Item = Result<TopicHandshakeMessage<T>, TopicHandshakeError<T>>> + Unpin,
) {
    let topic_sink = sink
        .sink_map_err(|err| TopicHandshakeError::MessageSink(format!("{err:?}")))
        .with(|message| {
            ready(Ok::<_, TopicHandshakeError<T>>(TopicLogSyncMessage::<
                T,
                L,
                E,
            >::Handshake(
                message
            )))
        });
    let topic_stream = stream.by_ref().map(|message| {
        let message =
            message.map_err(|err| TopicHandshakeError::MessageStream(format!("{err:?}")))?;
        match message {
            TopicLogSyncMessage::Handshake(message) => Ok(message),
            TopicLogSyncMessage::Sync(_) | TopicLogSyncMessage::Live(_, _) => Err(
                TopicHandshakeError::MessageStream("non-protocol message received".to_string()),
            ),
        }
    });
    (topic_sink, topic_stream)
}

/// Map raw message sink and stream into log sync protocol specific channels.
#[allow(clippy::complexity)]
pub(crate) fn sync_channels<'a, T, L, E, S>(
    sink: &mut (impl Sink<TopicLogSyncMessage<T, L, E>, Error = CborCodecError> + Unpin),
    stream: &mut (impl Stream<Item = Result<TopicLogSyncMessage<T, L, E>, CborCodecError>> + Unpin),
) -> (
    impl Sink<LogSyncMessage<L>, Error = LogSyncError<L, E, S>> + Unpin,
    impl Stream<Item = Result<LogSyncMessage<L>, LogSyncError<L, E, S>>> + Unpin,
)
where
    T: Debug,
    E: Debug,
    S: LogStore<L, E>,
{
    let log_sync_sink = sink
        .sink_map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))
        .with(|message| {
            ready(Ok::<_, LogSyncError<L, E, S>>(
                TopicLogSyncMessage::<T, L, E>::Sync(message),
            ))
        });
    let log_sync_stream = stream.by_ref().map(|message| {
        let message = message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;
        match message {
            TopicLogSyncMessage::Sync(message) => Ok(message),
            TopicLogSyncMessage::Handshake(_) | TopicLogSyncMessage::Live(_, _) => Err(
                LogSyncError::MessageStream("non-protocol message received".to_string()),
            ),
        }
    });

    (log_sync_sink, log_sync_stream)
}

/// Error type occurring in topic log sync protocol.
#[derive(Debug, Error)]
pub enum TopicLogSyncError<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E>,
    M: TopicLogMap<T, L>,
{
    #[error(transparent)]
    TopicHandshake(#[from] TopicHandshakeError<T>),

    #[error(transparent)]
    Sync(#[from] LogSyncError<L, E, S>),

    #[error(transparent)]
    TopicMap(M::Error),

    #[error(transparent)]
    Codec(#[from] CborCodecError),
}

/// Topic log sync live-mode message types.
#[derive(Clone, Debug)]
pub enum LiveModeMessage<E> {
    FromSub {
        header: Header<E>,
        body: Option<Body>,
    },
    FromSync {
        header: Header<E>,
        body: Option<Body>,
    },
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
        header: Header<E>,
        body: Option<Body>,
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
#[allow(clippy::large_enum_variant)]
pub enum TopicLogSyncMessage<T, L, E> {
    Handshake(TopicHandshakeMessage<T>),
    Sync(LogSyncMessage<L>),
    Live(Header<E>, Option<Body>),
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
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use assert_matches::assert_matches;
    use futures::channel::mpsc;
    use futures::{AsyncWriteExt, StreamExt};
    use p2panda_core::cbor::encode_cbor;
    use p2panda_core::{Body, Header, PrivateKey, PublicKey};
    use p2panda_store::{LogStore, MemoryStore, OperationStore};
    use rand::Rng;
    use rand::rngs::StdRng;
    use tokio::io::{DuplexStream, ReadHalf, WriteHalf};
    use tokio::sync::broadcast;
    use tokio::task::LocalSet;
    use tokio::time::sleep;
    use tokio_util::compat::{Compat, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::cbor::into_cbor_stream;
    use crate::log_sync::{LogSyncEvent, LogSyncMessage, Metrics, Operation, StatusEvent};
    use crate::test_utils::{LogHeightTopic, LogHeightTopicMap, create_operation};
    use crate::topic_handshake::{TopicHandshakeEvent, TopicHandshakeMessage};
    use crate::topic_log_sync::{
        LiveModeMessage, Role, TopicLogSync, TopicLogSyncError, TopicLogSyncEvent,
    };
    use crate::traits::SyncProtocol;

    use super::TopicLogSyncMessage;

    type TestMessage = TopicLogSyncMessage<LogHeightTopic, u64, ()>;
    type TestTopicMap = LogHeightTopicMap<LogHeightTopic>;
    type TestTopicLogSync = TopicLogSync<LogHeightTopic, MemoryStore<u64>, TestTopicMap, u64, ()>;
    type TestLogSyncError =
        TopicLogSyncError<LogHeightTopic, MemoryStore<u64>, TestTopicMap, u64, ()>;

    pub struct TopicLogSyncPeer {
        pub store: MemoryStore<u64>,
        pub private_key: PrivateKey,
        pub read: Option<Compat<ReadHalf<DuplexStream>>>,
        pub write: Option<Compat<WriteHalf<DuplexStream>>>,
        pub topic_map: TestTopicMap,
    }

    impl TopicLogSyncPeer {
        pub fn new(peer_id: u64, stream: DuplexStream) -> Self {
            let store = MemoryStore::default();
            let topic_map = TestTopicMap::new();
            let mut rng = <StdRng as rand::SeedableRng>::seed_from_u64(peer_id);
            let private_key = PrivateKey::from_bytes(&rng.random());
            let (read, write) = tokio::io::split(stream);
            Self {
                store,
                private_key,
                read: Some(read.compat()),
                write: Some(write.compat_write()),
                topic_map,
            }
        }

        pub fn id(&self) -> PublicKey {
            self.private_key.public_key()
        }

        pub async fn send(&mut self, messages: &[TestMessage]) {
            let mut writer = self.write.take().unwrap();
            for message in messages {
                writer
                    .write(&encode_cbor(message).unwrap()[..])
                    .await
                    .unwrap();
            }
            self.write.replace(writer);
        }

        pub async fn recv_all(&mut self) -> Vec<TestMessage> {
            let idle_timeout = Duration::from_millis(200);
            let mut messages = vec![];
            let _ = self.write.take();
            let mut read = self.read.take().unwrap();
            let mut stream = into_cbor_stream(&mut read);
            loop {
                let timeout = sleep(idle_timeout);
                tokio::select! {
                    biased;
                    Some(Ok(message)) = stream.next() => messages.push(message),
                    _ = timeout =>  break
                }
            }
            drop(stream);
            self.read.replace(read);
            messages
        }

        pub async fn session(
            &mut self,
            role: Role<LogHeightTopic>,
            live_mode: bool,
        ) -> (
            TestTopicLogSync,
            mpsc::Receiver<TopicLogSyncEvent<LogHeightTopic, ()>>,
            broadcast::Sender<LiveModeMessage<()>>,
        ) {
            let (event_tx, event_rx) = mpsc::channel(128);
            let (live_tx, live_rx) = broadcast::channel(128);
            let live_rx = if live_mode { Some(live_rx) } else { None };
            let session = TopicLogSync::new(
                self.store.clone(),
                self.topic_map.clone(),
                role,
                live_rx,
                event_tx,
            );
            (session, event_rx, live_tx)
        }

        pub async fn create_operation(&mut self, body: &Body, log_id: u64) -> (Header, Vec<u8>) {
            let (seq_num, backlink) = self
                .store
                .latest_operation(&self.private_key.public_key(), &log_id)
                .await
                .unwrap()
                .map(|(header, _)| (header.seq_num + 1, Some(header.hash())))
                .unwrap_or((0, None));

            let (header, header_bytes) =
                create_operation(&self.private_key, body, seq_num, seq_num, backlink);

            self.store
                .insert_operation(header.hash(), &header, Some(body), &header_bytes, &log_id)
                .await
                .unwrap();
            (header, header_bytes)
        }

        pub fn insert_topic(
            &mut self,
            topic: &LogHeightTopic,
            logs: &HashMap<PublicKey, Vec<u64>>,
        ) {
            self.topic_map.insert(topic, logs.to_owned());
        }

        pub async fn run(&mut self, session: TestTopicLogSync) -> Result<(), TestLogSyncError> {
            let mut read = self.read.take().unwrap();
            let mut write = self.write.take().unwrap();
            session.run(&mut write, &mut read).await?;
            self.read.replace(read);
            self.write.replace(write);
            Ok(())
        }
    }

    #[tokio::test]
    async fn sync_session_no_operations() {
        let topic = LogHeightTopic::new("messages");
        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let mut peer_a = TopicLogSyncPeer::new(0, peer_a_stream);
        let mut peer_b = TopicLogSyncPeer::new(1, peer_b_stream);

        peer_b
            .send(&[
                TestMessage::Handshake(TopicHandshakeMessage::Done),
                TestMessage::Sync(LogSyncMessage::Have(vec![])),
                TestMessage::Sync(LogSyncMessage::Done),
            ])
            .await;

        let (session, mut events_rx, _) =
            peer_a.session(Role::Initiate(topic.clone()), false).await;
        peer_a.run(session).await.unwrap();

        let mut index = 0;
        while let Some(event) = events_rx.next().await {
            match index {
                0 => {
                    assert_eq!(
                        event,
                        TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Initiate(topic.clone()))
                    )
                }
                1 => assert_eq!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                2 => assert_matches!(
                    event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }),)
                    );
                }
                5 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync (
                            LogSyncEvent::Status(StatusEvent::Completed { metrics: Metrics { total_operations_remote, total_bytes_remote, .. } }),
                        ) => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let messages = peer_b.recv_all().await;

        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestMessage::Handshake(TopicHandshakeMessage::Topic(topic.clone()))
                ),
                1 => assert_eq!(message, TestMessage::Handshake(TopicHandshakeMessage::Done)),
                2 => assert_eq!(message, TestMessage::Sync(LogSyncMessage::Have(vec![]))),
                3 => assert_eq!(message, TestMessage::Sync(LogSyncMessage::Done)),
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn sync_operations_accept() {
        let log_id = 0;
        let topic = LogHeightTopic::new("messages");

        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let mut peer_a = TopicLogSyncPeer::new(0, peer_a_stream);
        let mut peer_b = TopicLogSyncPeer::new(1, peer_b_stream);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (_, header_bytes_0) = peer_a.create_operation(&body, log_id).await;
        let (_, header_bytes_1) = peer_a.create_operation(&body, log_id).await;
        let (_, header_bytes_2) = peer_a.create_operation(&body, log_id).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        peer_b
            .send(&[
                TestMessage::Handshake(TopicHandshakeMessage::Topic(topic.clone())),
                TestMessage::Handshake(TopicHandshakeMessage::Done),
                TestMessage::Sync(LogSyncMessage::Have(vec![])),
                TestMessage::Sync(LogSyncMessage::Done),
            ])
            .await;

        let (session, mut events_rx, _) = peer_a.session(Role::Accept, false).await;
        peer_a.run(session).await.unwrap();

        let mut index = 0;
        while let Some(event) = events_rx.next().await {
            match index {
                0 => {
                    assert_eq!(
                        event,
                        TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                    )
                }
                1 => assert_eq!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(topic.clone()))
                ),
                2 => assert_matches!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }),)
                    );
                }
                6 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync (
                            LogSyncEvent::Status(StatusEvent::Completed { metrics: Metrics { total_operations_remote, total_bytes_remote, .. } }),
                        ) => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let messages = peer_b.recv_all().await;
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(message, TestMessage::Handshake(TopicHandshakeMessage::Done),),
                1 => assert_eq!(
                    message,
                    TestMessage::Sync(LogSyncMessage::Have(vec![(peer_a.id(), vec![(0, 2)])]))
                ),
                2 => assert_eq!(
                    message,
                    TestMessage::Sync(LogSyncMessage::PreSync {
                        total_operations: 3,
                        total_bytes: 1027
                    })
                ),
                3 => {
                    let (header, body_inner) = assert_matches!(message, TestMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_0);
                    assert_eq!(Body::new(&body_inner), body)
                }
                4 => {
                    let (header, body_inner) = assert_matches!(message, TestMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_1);
                    assert_eq!(Body::new(&body_inner), body)
                }
                5 => {
                    let (header, body_inner) = assert_matches!(message, TestMessage::Sync(LogSyncMessage::Operation(
                    header,
                    Some(body),
                )) => (header, body));
                    assert_eq!(header, header_bytes_2);
                    assert_eq!(Body::new(&body_inner), body)
                }
                6 => assert_eq!(message, TestMessage::Sync(LogSyncMessage::Done)),
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn topic_log_sync_full_duplex() {
        let topic = LogHeightTopic::new("messages");
        let log_id = 0;

        let (a_stream, b_stream) = tokio::io::duplex(64 * 1024);
        let mut peer_a = TopicLogSyncPeer::new(0, a_stream);
        let mut peer_b = TopicLogSyncPeer::new(1, b_stream);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, _) = peer_a.create_operation(&body, 0).await;
        let (header_1, _) = peer_a.create_operation(&body, 0).await;
        let (header_2, _) = peer_a.create_operation(&body, 0).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        let (peer_a_session, mut peer_a_events_rx, _) =
            peer_a.session(Role::Initiate(topic.clone()), false).await;

        let (peer_b_session, mut peer_b_events_rx, _) = peer_b.session(Role::Accept, false).await;

        let local = LocalSet::new();
        local
            .run_until(async move {
                let peer_a_task =
                    tokio::task::spawn_local(
                        async move { peer_a.run(peer_a_session).await.unwrap() },
                    );
                let peer_b_task =
                    tokio::task::spawn_local(
                        async move { peer_b.run(peer_b_session).await.unwrap() },
                    );
                tokio::try_join!(peer_a_task, peer_b_task).unwrap()
            })
            .await;

        let mut index = 0;
        while let Some(event) = peer_a_events_rx.next().await {
            match index {
                0 => assert_matches!(
                    event,
                    TopicLogSyncEvent::<LogHeightTopic, ()>::Handshake(TopicHandshakeEvent::Initiate(
                        sent_topic,
                    ))
                    if sent_topic == topic
                ),
                1 => assert_eq!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                2 => assert_matches!(
                    event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let mut index = 0;
        while let Some(event) = peer_b_events_rx.next().await {
            match index {
                0 => assert_eq!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                ),
                1 => assert_matches!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(received_topic))
                    if received_topic == topic
                ),
                2 => assert_eq!(
                    event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }))
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }))
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }))
                    );
                }
                6 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TopicLogSyncEvent::Sync (
                        LogSyncEvent::Data(Operation {
                            header,
                            body,
                        }),
                    ) => (header, body));
                    assert_eq!(header, header_0);
                    assert_eq!(body_inner.unwrap(), body);
                }
                7 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TopicLogSyncEvent::Sync (
                        LogSyncEvent::Data(Operation {
                            header,
                            body,
                        }),
                    ) => (header, body));
                    assert_eq!(header, header_1);
                    assert_eq!(body_inner.unwrap(), body);
                }
                8 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TopicLogSyncEvent::Sync (
                        LogSyncEvent::Data(Operation {
                            header,
                            body,
                        }),
                    ) => (header, body));
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner.unwrap(), body);
                }
                9 => {
                    assert_matches!(
                        event,
                        TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }
    }
}
