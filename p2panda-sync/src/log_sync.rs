// SPDX-License-Identifier: AGPL-3.0-or-later

//! Efficient sync protocol for append-only log data types.
//!
//! This implementation is generic over the actual data type implementation, as long as it follows
//! the form of a numbered, linked list it will be compatible for sync. p2panda provides an own log
//! implementation in `p2panda-core` which can be easily combined with this sync protocol.
//!
//! The protocol checks the current local "log heights", that is the index of the latest known
//! entry in each log, of the "initiating" peer and sends them in form of a "Have" message to the
//! remote one. The "accepting", remote peer matches the given log heights with the locally present
//! ones, calculates the delta of missing entries and sends them to the initiating peer as part of
//! "Data" messages.
//!
//! In this implementation the "accepting" peer will never receive any new data from the
//! "initiating" counter-part. Usually this would be handled in a "reverse" sync session which
//! might run concurrently.
//!
//! To find out which logs to send matching the given "topic" a `TopicMap` is provided. This
//! interface aids the sync protocol in deciding which logs to transfer for each given topic. Due
//! to the generic nature of `TopicMap` it's possible to realize very different applications on top
//! of the same sync implementation.
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{stream, AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use p2panda_core::{Extensions, PublicKey};
use p2panda_store::{LogId, LogStore};
use serde::{Deserialize, Serialize};

use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::{FromSync, SyncError, SyncProtocol, Topic, TopicMap};

type SeqNum = u64;

type LogHeights<T> = Vec<(T, SeqNum)>;

type Logs<T> = HashMap<PublicKey, Vec<T>>;

/// Messages to be sent over the wire between the two peers.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
enum Message<T, L = String> {
    Have(T, Vec<(PublicKey, LogHeights<L>)>),
    Data(Vec<u8>, Option<Vec<u8>>),
    Done,
}

/// Efficient sync protocol for append-only log data types.
#[derive(Clone, Debug)]
pub struct LogSyncProtocol<TM, L, E, S: LogStore<L, E>> {
    topic_map: TM,
    store: S,
    _marker: PhantomData<(L, E)>,
}

impl<TM, L, E, S> LogSyncProtocol<TM, L, E, S>
where
    S: LogStore<L, E>,
{
    /// Returns a new sync protocol instance, configured with a store and `TopicMap` implementation
    /// which associates the to-be-synced logs with a given topic.
    pub fn new(topic_map: TM, store: S) -> Self {
        Self {
            topic_map,
            store,
            _marker: PhantomData {},
        }
    }
}

#[async_trait]
impl<'a, T, TM, L, E, S> SyncProtocol<T, 'a> for LogSyncProtocol<TM, L, E, S>
where
    T: Topic,
    TM: TopicMap<T, Logs<L>>,
    L: LogId + for<'de> Deserialize<'de> + Serialize + 'a,
    E: Extensions + Send + Sync + 'a,
    S: Debug + Sync + LogStore<L, E>,
{
    fn name(&self) -> &'static str {
        "p2panda-log-sync-v1"
    }

    async fn initiate(
        self: Arc<Self>,
        topic: T,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_received = false;

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        // Get the log ids which are associated with this topic.
        let Some(logs) = self.topic_map.get(&topic).await else {
            return Err(SyncError::Critical(format!("unknown {topic:?} topic")));
        };

        // Get local log heights for all authors who have published under the requested log ids.
        let mut local_log_heights = Vec::new();
        for (public_key, log_ids) in logs {
            let mut log_heights = Vec::new();
            for log_id in log_ids {
                let latest = self
                    .store
                    .latest_operation(&public_key, &log_id)
                    .await
                    .map_err(|err| {
                        SyncError::Critical(format!("can't retrieve log heights from store, {err}"))
                    })?;

                if let Some((header, _)) = latest {
                    log_heights.push((log_id.clone(), header.seq_num));
                };
            }
            local_log_heights.push((public_key, log_heights));
        }

        // Send our `Have` message to the remote peer.
        sink.send(Message::<T, L>::Have(
            topic.clone(),
            local_log_heights.clone(),
        ))
        .await?;

        // As we initiated this sync session we are done after sending the `Have` message.
        sink.send(Message::Done).await?;

        // Announce the topic of the sync session to the app layer.
        app_tx.send(FromSync::HandshakeSuccess(topic)).await?;

        // Consume messages arriving on the receive stream.
        while let Some(result) = stream.next().await {
            let message: Message<L> = result?;

            match message {
                Message::Have(_, _) => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected \"have\" message received".to_string(),
                    ))
                }
                Message::Data(header, payload) => {
                    // Forward data received from the remote to the app layer.
                    app_tx.send(FromSync::Data { header, payload }).await?;
                }
                Message::Done => {
                    sync_done_received = true;
                }
            };

            if sync_done_received {
                break;
            }
        }

        // Flush all bytes so that no messages are lost.
        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        while let Some(result) = stream.next().await {
            let message: Message<T, L> = result?;
            match &message {
                Message::Have(topic, remote_log_heights) => {
                    // Signal that the "handshake" phase of this protocol is complete as we
                    // received the topic.
                    app_tx
                        .send(FromSync::HandshakeSuccess(topic.clone()))
                        .await?;

                    // Get the log ids which are associated with this topic.
                    let Some(logs) = self.topic_map.get(topic).await else {
                        return Err(SyncError::UnexpectedBehaviour(format!(
                            "unknown topic {topic:?} requested from remote peer"
                        )));
                    };

                    let remote_log_heights_map: HashMap<PublicKey, Vec<(L, u64)>> =
                        remote_log_heights.clone().into_iter().collect();

                    // Now that the topic has been translated into a collection of logs we want to
                    // compare our own local log heights with what the remote sent for this topic.
                    //
                    // If our logs are more advanced for any log we should send the missing
                    // entries.
                    for (public_key, log_ids) in logs {
                        for log_id in log_ids {
                            // For all logs in this topic scope get the local height.
                            let latest_operation = self
                                .store
                                .latest_operation(&public_key, &log_id)
                                .await
                                .map_err(|err| {
                                    SyncError::Critical(format!(
                                        "can't retreive log heights from store, {err}"
                                    ))
                                })?;

                            let log_height = match latest_operation {
                                Some((header, _)) => header.seq_num,
                                // If we don't have this log then continue onto the next without
                                // sending any messages.
                                None => continue,
                            };

                            // Calculate from which seq num in the log the remote needs operations.
                            let remote_needs_from = match remote_log_heights_map.get(&public_key) {
                                Some(log_heights) => {
                                    match log_heights.iter().find(|(id, _)| *id == log_id) {
                                        // The log is known by the remote, take their log height
                                        // and plus one.
                                        Some((_, log_height)) => log_height + 1,
                                        // The log is not known, they need from seq num 0
                                        None => 0,
                                    }
                                }
                                // The author is not known, they need from seq num 0.
                                None => 0,
                            };

                            // If we have operations the remote needs then send them now.
                            if remote_needs_from <= log_height {
                                let messages: Vec<Message<T, L>> = remote_needs(
                                    &self.store,
                                    &log_id,
                                    &public_key,
                                    remote_needs_from,
                                )
                                .await?;
                                sink.send_all(&mut stream::iter(messages.into_iter().map(Ok)))
                                    .await?;
                            };
                        }
                    }

                    // As we have processed the remotes `Have` message then we are "done" from
                    // this end.
                    sink.send(Message::Done).await?;
                    sync_done_sent = true;
                }
                Message::Data(_, _) => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected \"data\" message received".to_string(),
                    ));
                }
                Message::Done => {
                    sync_done_received = true;
                }
            };

            if sync_done_received && sync_done_sent {
                break;
            }
        }

        // Flush all bytes so that no messages are lost.
        sink.flush().await?;
        app_tx.flush().await?;

        Ok(())
    }
}

/// Helper method for getting only the operations a peer needs from a log and composing them into
/// the expected message format.
async fn remote_needs<T, L, E>(
    store: &impl LogStore<L, E>,
    log_id: &L,
    public_key: &PublicKey,
    from: SeqNum,
) -> Result<Vec<Message<T, L>>, SyncError>
where
    E: Extensions + Send + Sync,
{
    let log = store
        .get_raw_log(public_key, log_id, Some(from))
        .await
        .map_err(|err| SyncError::Critical(format!("could not retrieve log from store, {err}")))?;

    let messages = log
        .unwrap_or_default()
        .into_iter()
        .map(|(header, payload)| Message::Data(header, payload))
        .collect();

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures::SinkExt;
    use p2panda_core::extensions::DefaultExtensions;
    use p2panda_core::{Body, Hash, Header, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore};
    use serde::{Deserialize, Serialize};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tokio_util::sync::PollSender;

    use crate::{FromSync, SyncError, SyncProtocol, Topic};

    use super::{LogSyncProtocol, Logs, Message, TopicMap};

    impl<T, L> Message<T, L>
    where
        T: Serialize,
        L: Serialize,
    {
        pub fn to_bytes(&self) -> Vec<u8> {
            p2panda_core::cbor::encode_cbor(&self).expect("type can be serialized")
        }
    }

    fn create_operation<E: Clone + Serialize>(
        private_key: &PrivateKey,
        body: &Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
        extensions: Option<E>,
    ) -> (Hash, Header<E>, Vec<u8>) {
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp,
            seq_num,
            backlink,
            previous: vec![],
            extensions,
        };
        header.sign(&private_key);
        let header_bytes = header.to_bytes();
        (header.hash(), header, header_bytes)
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
    pub struct LogHeightTopic(String, [u8; 32]);

    impl LogHeightTopic {
        pub fn new(name: &str) -> Self {
            Self(name.to_owned(), [0; 32])
        }
    }

    impl Topic for LogHeightTopic {}

    #[derive(Clone, Debug)]
    struct LogHeightTopicMap<T>(HashMap<T, Logs<u64>>);

    impl<T> LogHeightTopicMap<T>
    where
        T: Topic,
    {
        pub fn new() -> Self {
            LogHeightTopicMap(HashMap::new())
        }

        fn insert(&mut self, topic: &T, logs: Logs<u64>) -> Option<Logs<u64>> {
            self.0.insert(topic.clone(), logs)
        }
    }

    #[async_trait]
    impl<T> TopicMap<T, Logs<u64>> for LogHeightTopicMap<T>
    where
        T: Topic,
    {
        async fn get(&self, topic: &T) -> Option<Logs<u64>> {
            self.0.get(topic).cloned()
        }
    }

    async fn assert_message_bytes(
        mut rx: ReadHalf<DuplexStream>,
        messages: Vec<Message<LogHeightTopic>>,
    ) {
        let mut buf = Vec::new();
        rx.read_to_end(&mut buf).await.unwrap();
        assert_eq!(
            buf,
            messages.iter().fold(Vec::new(), |mut acc, message| {
                acc.extend(message.to_bytes());
                acc
            })
        );
    }

    fn to_bytes(messages: Vec<Message<LogHeightTopic>>) -> Vec<u8> {
        messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        })
    }

    #[tokio::test]
    async fn sync_no_operations_accept() {
        let topic = LogHeightTopic::new("messages");
        let logs = HashMap::new();
        let store = MemoryStore::<u64, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = to_bytes(vec![Message::Have(topic.clone(), vec![]), Message::Done]);
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol::new(topic_map, store));
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let _ = protocol
            .accept(
                Box::new(&mut peer_a_write.compat_write()),
                Box::new(&mut peer_a_read.compat()),
                Box::new(&mut sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(peer_b_read, vec![Message::Done]).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![FromSync::HandshakeSuccess(topic)])
    }

    #[tokio::test]
    async fn sync_no_operations_open() {
        let topic = LogHeightTopic::new("messages");
        let logs = HashMap::new();
        let store = MemoryStore::<u64, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = vec![Message::<String>::Done.to_bytes()].concat();
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol::new(topic_map, store));
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|err| crate::SyncError::Critical(err.to_string()));
        let _ = protocol
            .initiate(
                topic.clone(),
                Box::new(&mut peer_a_write.compat_write()),
                Box::new(&mut peer_a_read.compat()),
                Box::new(&mut sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(
            peer_b_read,
            vec![Message::Have(topic.clone(), vec![]), Message::Done],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![FromSync::HandshakeSuccess(topic)])
    }

    #[tokio::test]
    async fn sync_operations_accept() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = LogHeightTopic::new("messages");
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        let mut store = MemoryStore::<u64, DefaultExtensions>::new();

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body, 0, 0, None, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body, 1, 100, Some(hash_0), None);
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body, 2, 200, Some(hash_1), None);

        store
            .insert_operation(hash_0, &header_0, Some(&body), &header_bytes_0, &log_id)
            .await
            .unwrap();
        store
            .insert_operation(hash_1, &header_1, Some(&body), &header_bytes_1, &log_id)
            .await
            .unwrap();
        store
            .insert_operation(hash_2, &header_2, Some(&body), &header_bytes_2, &log_id)
            .await
            .unwrap();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let messages = vec![
            Message::Have::<LogHeightTopic>(topic.clone(), vec![]),
            Message::Done,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol::new(topic_map, store));
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let _ = protocol
            .accept(
                Box::new(&mut peer_a_write.compat_write()),
                Box::new(&mut peer_a_read.compat()),
                Box::new(&mut sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        let messages = vec![
            Message::Data(header_bytes_0, Some(body.to_bytes())),
            Message::Data(header_bytes_1, Some(body.to_bytes())),
            Message::Data(header_bytes_2, Some(body.to_bytes())),
            Message::Done,
        ];
        assert_message_bytes(peer_b_read, messages).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, [FromSync::HandshakeSuccess(topic)])
    }

    #[tokio::test]
    async fn sync_operations_open() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = LogHeightTopic::new("messages");
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        let store = MemoryStore::<u64, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Create operations which will be sent to peer a
        let body = Body::new("Hello, Sloth!".as_bytes());

        let (hash_0, _, header_bytes_0) =
            create_operation::<DefaultExtensions>(&private_key, &body, 0, 0, None, None);
        let (hash_1, _, header_bytes_1) =
            create_operation::<DefaultExtensions>(&private_key, &body, 1, 100, Some(hash_0), None);
        let (_, _, header_bytes_2) =
            create_operation::<DefaultExtensions>(&private_key, &body, 2, 200, Some(hash_1), None);

        // Write some message into peer_b's send buffer
        let messages: Vec<Message<String>> = vec![
            Message::Data(header_bytes_0.clone(), Some(body.to_bytes())),
            Message::Data(header_bytes_1.clone(), Some(body.to_bytes())),
            Message::Data(header_bytes_2.clone(), Some(body.to_bytes())),
            Message::Done,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol::new(topic_map, store));
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let _ = protocol
            .initiate(
                topic.clone(),
                Box::new(&mut peer_a_write.compat_write()),
                Box::new(&mut peer_a_read.compat()),
                Box::new(&mut sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(
            peer_b_read,
            vec![
                Message::Have(topic.clone(), vec![(private_key.public_key(), vec![])]),
                Message::Done,
            ],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(
            messages,
            [
                FromSync::HandshakeSuccess(topic),
                FromSync::Data {
                    header: header_bytes_0,
                    payload: Some(body.to_bytes())
                },
                FromSync::Data {
                    header: header_bytes_1,
                    payload: Some(body.to_bytes())
                },
                FromSync::Data {
                    header: header_bytes_2,
                    payload: Some(body.to_bytes())
                },
            ]
        );
    }

    #[tokio::test]
    async fn e2e_sync() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = LogHeightTopic::new("messages");
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        // Create an empty store for peer a
        let store_1 = MemoryStore::default();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let peer_a_protocol = Arc::new(LogSyncProtocol::new(topic_map.clone(), store_1));

        // Create a store for peer b and populate it with 3 operations
        let mut store_2 = MemoryStore::default();
        let body = Body::new("Hello, Sloth!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body, 0, 0, None, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body, 1, 100, Some(hash_0), None);
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body, 2, 200, Some(hash_1), None);

        store_2
            .insert_operation(hash_0, &header_0, Some(&body), &header_bytes_0, &log_id)
            .await
            .unwrap();
        store_2
            .insert_operation(hash_1, &header_1, Some(&body), &header_bytes_1, &log_id)
            .await
            .unwrap();
        store_2
            .insert_operation(hash_2, &header_2, Some(&body), &header_bytes_2, &log_id)
            .await
            .unwrap();

        // Construct b log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogSyncProtocol::new(topic_map, store_2));

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink =
            PollSender::new(peer_a_app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let topic_clone = topic.clone();
        let handle_1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .initiate(
                    topic_clone,
                    Box::new(&mut peer_a_write.compat_write()),
                    Box::new(&mut peer_a_read.compat()),
                    Box::new(&mut sink),
                )
                .await
                .unwrap();
        });

        // Spawn a task which accepts a sync session on peer b runs it to completion
        let peer_b_protocol_clone = peer_b_protocol.clone();
        let (peer_b_app_tx, mut peer_b_app_rx) = mpsc::channel(128);
        let mut sink =
            PollSender::new(peer_b_app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let handle_2 = tokio::spawn(async move {
            peer_b_protocol_clone
                .accept(
                    Box::new(&mut peer_b_write.compat_write()),
                    Box::new(&mut peer_b_read.compat()),
                    Box::new(&mut sink),
                )
                .await
                .unwrap();
        });

        // Wait for both to complete
        let (_, _) = tokio::join!(handle_1, handle_2);

        let peer_a_expected_messages = vec![
            FromSync::HandshakeSuccess(topic.clone()),
            FromSync::Data {
                header: header_bytes_0,
                payload: Some(body.to_bytes()),
            },
            FromSync::Data {
                header: header_bytes_1,
                payload: Some(body.to_bytes()),
            },
            FromSync::Data {
                header: header_bytes_2,
                payload: Some(body.to_bytes()),
            },
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;
        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![FromSync::HandshakeSuccess(topic.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;
        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }

    #[tokio::test]
    async fn e2e_partial_sync() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = LogHeightTopic::new("messages");
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        let body = Body::new("Hello, Sloth!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body, 0, 0, None, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body, 1, 100, Some(hash_0), None);
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body, 2, 200, Some(hash_1), None);

        let mut store_1 = MemoryStore::default();
        store_1
            .insert_operation(hash_0, &header_0, Some(&body), &header_bytes_0, &log_id)
            .await
            .unwrap();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let peer_a_protocol = Arc::new(LogSyncProtocol::new(topic_map.clone(), store_1));

        // Create a store for peer b and populate it with 3 operations
        let mut store_2 = MemoryStore::default();

        // Insert these operations to the store
        store_2
            .insert_operation(hash_0, &header_0, Some(&body), &header_bytes_0, &log_id)
            .await
            .unwrap();
        store_2
            .insert_operation(hash_1, &header_1, Some(&body), &header_bytes_1, &log_id)
            .await
            .unwrap();
        store_2
            .insert_operation(hash_2, &header_2, Some(&body), &header_bytes_2, &log_id)
            .await
            .unwrap();

        // Construct a log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogSyncProtocol::new(topic_map, store_2));

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink =
            PollSender::new(peer_a_app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let topic_clone = topic.clone();
        let handle_1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .initiate(
                    topic_clone,
                    Box::new(&mut peer_a_write.compat_write()),
                    Box::new(&mut peer_a_read.compat()),
                    Box::new(&mut sink),
                )
                .await
                .unwrap();
        });

        // Spawn a task which accepts a sync session on peer b runs it to completion
        let peer_b_protocol_clone = peer_b_protocol.clone();
        let (peer_b_app_tx, mut peer_b_app_rx) = mpsc::channel(128);
        let mut sink =
            PollSender::new(peer_b_app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let handle_2 = tokio::spawn(async move {
            peer_b_protocol_clone
                .accept(
                    Box::new(&mut peer_b_write.compat_write()),
                    Box::new(&mut peer_b_read.compat()),
                    Box::new(&mut sink),
                )
                .await
                .unwrap();
        });

        // Wait for both to complete
        let (_, _) = tokio::join!(handle_1, handle_2);

        let peer_a_expected_messages = vec![
            FromSync::HandshakeSuccess(topic.clone()),
            FromSync::Data {
                header: header_bytes_1,
                payload: Some(body.to_bytes()),
            },
            FromSync::Data {
                header: header_bytes_2,
                payload: Some(body.to_bytes()),
            },
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;
        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![FromSync::HandshakeSuccess(topic.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;
        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }
}
