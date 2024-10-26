// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{stream, AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use p2panda_core::PublicKey;
use p2panda_store::{LogStore, MemoryStore};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::{FromSync, SyncError, SyncProtocol, TopicId, TopicMap};

type SeqNum = u64;
pub type LogHeights = Vec<(PublicKey, SeqNum)>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Message<T = String> {
    Have(TopicId, Vec<(T, LogHeights)>),
    Operation(Vec<u8>, Option<Vec<u8>>),
    SyncDone,
}

#[cfg(test)]
impl<T> Message<T>
where
    T: Serialize,
{
    pub fn to_bytes(&self) -> Vec<u8> {
        p2panda_core::cbor::encode_cbor(&self).expect("type can be serialized")
    }
}

static LOG_SYNC_PROTOCOL_NAME: &str = "p2panda/log_sync";

#[derive(Clone, Debug)]
pub struct LogSyncProtocol<S, T, E> {
    pub topic_map: S,
    pub store: MemoryStore<T, E>,
}

#[async_trait]
impl<'a, S, T, E> SyncProtocol<'a> for LogSyncProtocol<S, T, E>
where
    S: Debug + TopicMap<TopicId, T> + Send + Sync,
    T: Clone
        + Debug
        + Default
        + Eq
        + Hash
        + Send
        + Sync
        + for<'de> Deserialize<'de>
        + Serialize
        + 'a,
    E: Clone + Debug + Default + Send + Sync + for<'de> Deserialize<'de> + Serialize + 'a,
{
    fn name(&self) -> &'static str {
        LOG_SYNC_PROTOCOL_NAME
    }

    async fn initiate(
        self: Arc<Self>,
        topic: &TopicId,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_received = false;

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        // Get the log ids which are associated with this topic
        let Some(log_ids) = self.topic_map.get(topic).await else {
            return Err(SyncError::Critical(format!("unknown {topic:?} topic")));
        };

        // Get local log heights for all authors who have published under the requested log ids
        // @TODO: this will require changes soon when `get_log_heights` method includes the public
        // key as an argument.
        let mut local_log_heights = Vec::new();
        for log_id in log_ids {
            let log_heights = self.store.get_log_heights(&log_id).await.map_err(|err| {
                SyncError::Critical(format!("can't retreive log heights from store, {err}"))
            })?;
            local_log_heights.push((log_id, log_heights));
        }

        // Send our `Have` message to the remote peer
        sink.send(Message::<T>::Have(*topic, local_log_heights.clone()))
            .await?;

        // As we initiated this sync session we are done after sending the `Have` message
        sink.send(Message::SyncDone).await?;

        // Announce the topic of the sync session to the app layer
        app_tx.send(FromSync::Topic(*topic)).await?;

        // Consume messages arriving on the receive stream
        while let Some(result) = stream.next().await {
            let message: Message<T> = result?;
            debug!("message received: {:?}", message);

            match message {
                Message::Have(_, _) => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected \"have\" message received".to_string(),
                    ))
                }
                Message::Operation(header, payload) => {
                    // Forward data received from the remote to the app layer.
                    app_tx.send(FromSync::Data(header, payload)).await?;
                }
                Message::SyncDone => {
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

        debug!("sync session finished");

        Ok(())
    }

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        while let Some(result) = stream.next().await {
            let message: Message<T> = result?;
            debug!("message received: {:?}", message);

            match &message {
                Message::Have(topic, remote_log_heights) => {
                    // Announce the topic id that we received from the initiating peer.
                    app_tx.send(FromSync::Topic(*topic)).await?;

                    // Get the log ids which are associated with this topic.
                    let Some(log_ids) = self.topic_map.get(topic).await else {
                        return Err(SyncError::UnexpectedBehaviour(format!(
                            "unknown topic {topic:?} requested from remote peer"
                        )));
                    };

                    let remote_log_heights_map: HashMap<T, Vec<(PublicKey, u64)>> =
                        remote_log_heights.clone().into_iter().collect();

                    // For every log id we need to:
                    // * Retrieve the local log heights for all contributing authors
                    // * Compare our local log heights with those sent from the remote peer
                    // * Send any operations the remote peer is missing
                    for log_id in log_ids {
                        let local_log_heights =
                            self.store.get_log_heights(&log_id).await.map_err(|err| {
                                SyncError::Critical(format!(
                                    "can't retreive log heights from store, {err}"
                                ))
                            })?;

                        for (public_key, seq_num) in local_log_heights {
                            let Some(remote_log_heights) = remote_log_heights_map.get(&log_id)
                            else {
                                // The remote peer didn't request logs with this id
                                continue;
                            };

                            for (remote_pub_key, remote_seq_num) in remote_log_heights.iter() {
                                // Compare log heights sent by the remote with our local logs, if
                                // our logs are more advanced calculate and send operations the
                                // remote is missing
                                if *remote_pub_key == public_key && *remote_seq_num < seq_num {
                                    let messages = remote_needs(
                                        &self.store,
                                        &log_id,
                                        &public_key,
                                        *remote_seq_num + 1,
                                    )
                                    .await?;
                                    sink.send_all(&mut stream::iter(messages.into_iter().map(Ok)))
                                        .await?;
                                };
                            }

                            // If we know of an author the remote does not yet know about send all
                            // their operations
                            if !remote_log_heights
                                .iter()
                                .any(|(remote_public_key, _)| public_key == *remote_public_key)
                            {
                                let messages =
                                    remote_needs(&self.store, &log_id, &public_key, 0).await?;
                                sink.send_all(&mut stream::iter(messages.into_iter().map(Ok)))
                                    .await?;
                            }
                        }
                    }

                    // As we have processed the remotes `Have` message then we are "done" from
                    // this end
                    sink.send(Message::SyncDone).await?;
                    sync_done_sent = true;
                }
                Message::Operation(_, _) => {
                    return Err(SyncError::UnexpectedBehaviour(
                        "unexpected \"operation\" message received".to_string(),
                    ));
                }
                Message::SyncDone => {
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

        debug!("sync session finished");

        Ok(())
    }
}

// Helper method for getting only the operations a peer needs from a log and composing them into
// the expected message format.
async fn remote_needs<T, E>(
    store: &impl LogStore<T, E>,
    log_id: &T,
    public_key: &PublicKey,
    from: SeqNum,
) -> Result<Vec<Message<T>>, SyncError>
where
    E: Clone + Serialize,
{
    let log = store
        .get_raw_log(public_key, log_id)
        .await
        .map_err(|err| SyncError::Critical(format!("could not retrieve log from store, {err}")))?;

    let messages: Vec<Message<T>> = log
        .unwrap_or_default()
        .split_off(from as usize)
        .into_iter()
        .map(|(header, payload)| Message::Operation(header, payload))
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
    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tokio_util::sync::PollSender;

    use crate::{FromSync, SyncError, SyncProtocol, TopicId};

    use super::{LogSyncProtocol, Message, TopicMap};

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

    #[derive(Clone, Debug)]
    struct LogIdTopicMap(HashMap<TopicId, Vec<String>>);

    impl LogIdTopicMap {
        pub fn new() -> Self {
            Self(HashMap::new())
        }

        fn insert(&mut self, topic: TopicId, log_ids: Vec<String>) -> Option<Vec<String>> {
            self.0.insert(topic, log_ids)
        }
    }

    #[async_trait]
    impl TopicMap<TopicId, String> for LogIdTopicMap {
        async fn get(&self, topic: &TopicId) -> Option<Vec<String>> {
            self.0.get(topic).cloned()
        }
    }

    async fn assert_message_bytes(mut rx: ReadHalf<DuplexStream>, messages: Vec<Message<String>>) {
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

    fn to_bytes(messages: Vec<Message<String>>) -> Vec<u8> {
        messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        })
    }

    #[tokio::test]
    async fn sync_no_operations_accept() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        let log_id = String::from("messages");

        let store = MemoryStore::<String, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = to_bytes(vec![
            Message::Have(TOPIC_ID.clone(), vec![(log_id.clone(), vec![])]),
            Message::SyncDone,
        ]);
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, vec![log_id.clone()]);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
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
        assert_message_bytes(peer_b_read, vec![Message::SyncDone]).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![FromSync::Topic(TOPIC_ID)])
    }

    #[tokio::test]
    async fn sync_no_operations_open() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        let log_id = String::from("messages");

        let store = MemoryStore::<String, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = vec![Message::<String>::SyncDone.to_bytes()].concat();
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, vec![log_id.clone()]);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|err| crate::SyncError::Critical(err.to_string()));
        let _ = protocol
            .initiate(
                &TOPIC_ID,
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
                Message::Have(TOPIC_ID.clone(), vec![(log_id.clone(), vec![])]),
                Message::SyncDone,
            ],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![FromSync::Topic(TOPIC_ID)])
    }

    #[tokio::test]
    async fn sync_operations_accept() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];

        let log_id = String::from("messages");
        let private_key = PrivateKey::new();

        let mut store = MemoryStore::<String, DefaultExtensions>::new();

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
        let messages: Vec<Message<String>> = vec![
            Message::Have(TOPIC_ID.clone(), vec![(log_id.clone(), vec![])]),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, vec![log_id.clone()]);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
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
        let messages: Vec<Message<String>> = vec![
            Message::Operation(header_bytes_0, Some(body.to_bytes())),
            Message::Operation(header_bytes_1, Some(body.to_bytes())),
            Message::Operation(header_bytes_2, Some(body.to_bytes())),
            Message::SyncDone,
        ];
        assert_message_bytes(peer_b_read, messages).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, [FromSync::Topic(TOPIC_ID)])
    }

    #[tokio::test]
    async fn sync_operations_open() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];

        let log_id = String::from("messages");
        let store = MemoryStore::<String, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Create operations which will be sent to peer a
        let private_key = PrivateKey::new();
        let body = Body::new("Hello, Sloth!".as_bytes());

        let (hash_0, _, header_bytes_0) =
            create_operation::<DefaultExtensions>(&private_key, &body, 0, 0, None, None);
        let (hash_1, _, header_bytes_1) =
            create_operation::<DefaultExtensions>(&private_key, &body, 1, 100, Some(hash_0), None);
        let (_, _, header_bytes_2) =
            create_operation::<DefaultExtensions>(&private_key, &body, 2, 200, Some(hash_1), None);

        // Write some message into peer_b's send buffer
        let messages: Vec<Message<String>> = vec![
            Message::Operation(header_bytes_0.clone(), Some(body.to_bytes())),
            Message::Operation(header_bytes_1.clone(), Some(body.to_bytes())),
            Message::Operation(header_bytes_2.clone(), Some(body.to_bytes())),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, vec![log_id.clone()]);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let _ = protocol
            .initiate(
                &TOPIC_ID,
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
                Message::Have(TOPIC_ID.clone(), vec![(log_id.clone(), vec![])]),
                Message::SyncDone,
            ],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(
            messages,
            [
                FromSync::Topic(TOPIC_ID),
                FromSync::Data(header_bytes_0, Some(body.to_bytes())),
                FromSync::Data(header_bytes_1, Some(body.to_bytes())),
                FromSync::Data(header_bytes_2, Some(body.to_bytes())),
            ]
        );
    }

    #[tokio::test]
    async fn e2e_sync() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        let log_id = String::from("messages");

        // Create an empty store for peer a
        let store_1 = MemoryStore::default();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, vec![log_id.clone()]);
        let peer_a_protocol = Arc::new(LogSyncProtocol {
            topic_map: topic_map.clone(),
            store: store_1,
        });

        // Create a store for peer b and populate it with 3 operations
        let mut store_2 = MemoryStore::default();
        let private_key = PrivateKey::new();
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
        let peer_b_protocol = Arc::new(LogSyncProtocol {
            topic_map,
            store: store_2,
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink =
            PollSender::new(peer_a_app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let handle_1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .initiate(
                    &TOPIC_ID,
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
            FromSync::Topic(TOPIC_ID.clone()),
            FromSync::Data(header_bytes_0, Some(body.to_bytes())),
            FromSync::Data(header_bytes_1, Some(body.to_bytes())),
            FromSync::Data(header_bytes_2, Some(body.to_bytes())),
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;
        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![FromSync::Topic(TOPIC_ID.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;
        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }

    #[tokio::test]
    async fn e2e_partial_sync() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];

        let log_id = String::from("messages");
        let private_key = PrivateKey::new();
        let body = Body::new("Hello, Sloth!".as_bytes());

        let (hash_0, header_0, header_bytes_0) =
            create_operation(&private_key, &body, 0, 0, None, None);
        let (hash_1, header_1, header_bytes_1) =
            create_operation(&private_key, &body, 1, 100, Some(hash_0), None);
        let (hash_2, header_2, header_bytes_2) =
            create_operation(&private_key, &body, 2, 200, Some(hash_1), None);

        // Create a store for peer a and populate it with one operation
        let mut store_1 = MemoryStore::default();
        store_1
            .insert_operation(hash_0, &header_0, Some(&body), &header_bytes_0, &log_id)
            .await
            .unwrap();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, vec![log_id.clone()]);
        let peer_a_protocol = Arc::new(LogSyncProtocol {
            topic_map: topic_map.clone(),
            store: store_1,
        });

        // Create a store for peer b and populate it with 3 operations
        let mut store_2 = MemoryStore::default();

        // Insert these operations to the store using `TOPIC_ID` as the log id
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
        let peer_b_protocol = Arc::new(LogSyncProtocol {
            topic_map,
            store: store_2,
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink =
            PollSender::new(peer_a_app_tx).sink_map_err(|err| SyncError::Critical(err.to_string()));
        let handle_1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .initiate(
                    &TOPIC_ID,
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
            FromSync::Topic(TOPIC_ID.clone()),
            FromSync::Data(header_bytes_1, Some(body.to_bytes())),
            FromSync::Data(header_bytes_2, Some(body.to_bytes())),
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;
        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![FromSync::Topic(TOPIC_ID.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;
        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }
}
