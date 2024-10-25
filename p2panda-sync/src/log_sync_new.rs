// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{stream, AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use p2panda_core::PublicKey;
use p2panda_store::{LogStore, MemoryStore};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::{FromSync, SyncError, SyncProtocol, TopicMap};

type SeqNum = u64;
pub type LogHeights<T> = Vec<(T, SeqNum)>;
pub type Logs<T> = HashMap<PublicKey, Vec<T>>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Message<T, L = String> {
    Have(T, Vec<(PublicKey, LogHeights<L>)>),
    Operation(Vec<u8>, Option<Vec<u8>>),
    SyncDone,
}

#[cfg(test)]
impl<T, L> Message<T, L>
where
    T: Serialize,
    L: Serialize,
{
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(&self, &mut bytes).expect("type can be serialized");
        bytes
    }
}

static LOG_SYNC_PROTOCOL_NAME: &str = "p2panda/log_sync";

#[derive(Clone, Debug)]
pub struct LogSyncProtocol<TopicMap, LogId, Ext> {
    pub topic_map: TopicMap,
    pub store: MemoryStore<LogId, Ext>,
}

#[async_trait]
impl<'a, T, TM, L, E> SyncProtocol<T, 'a> for LogSyncProtocol<TM, L, E>
where
    T: Clone + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de>,
    TM: Debug + TopicMap<T, Logs<L>> + Send + Sync,
    L: Clone
        + Debug
        + Default
        + Eq
        + StdHash
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
            return Err(SyncError::Protocol("Unknown topic id".to_string()));
        };

        // Get local log heights for all authors who have published under the requested log ids.
        // @TODO: this will require changes soon when `get_log_heights` method includes the public
        // key as an argument.
        let mut local_log_heights = Vec::new();
        for (public_key, log_ids) in logs {
            let mut log_heights = Vec::new();
            for log_id in log_ids {
                let latest_operation = self
                    .store
                    .latest_operation(&public_key, &log_id)
                    .await
                    .expect("memory store error");

                if let Some(latest_operation) = latest_operation {
                    log_heights.push((log_id.clone(), latest_operation.header.seq_num));
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
        sink.send(Message::SyncDone).await?;

        // Announce the topic of the sync session to the app layer.
        app_tx.send(FromSync::Topic(topic)).await?;

        // Consume messages arriving on the receive stream.
        while let Some(result) = stream.next().await {
            let message: Message<L> = result?;
            debug!("message received: {:?}", message);

            match message {
                Message::Have(_, _) => {
                    return Err(SyncError::Protocol(
                        "unexpected Have message received".to_string(),
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
        mut app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_cbor_sink(tx);
        let mut stream = into_cbor_stream(rx);

        while let Some(result) = stream.next().await {
            let message: Message<T, L> = result?;
            debug!("message received: {:?}", message);

            match &message {
                Message::Have(topic, remote_log_heights) => {
                    // Announce the topic id that we received from the initiating peer.
                    app_tx.send(FromSync::Topic(topic.clone())).await?;

                    // Get the log ids which are associated with this topic.
                    let Some(logs) = self.topic_map.get(&topic).await else {
                        return Err(SyncError::Protocol("Unknown topic id".to_string()));
                    };

                    let remote_log_heights_map: HashMap<PublicKey, Vec<(L, u64)>> =
                        remote_log_heights.clone().into_iter().collect();

                    // For every log id we need to:
                    // - retrieve the local log heights for all contributing authors
                    // - compare our local log heights with those sent from the remote peer
                    // - send any operations the remote peer is missing
                    for (public_key, log_ids) in logs {
                        let Some(remote_log_heights) = remote_log_heights_map.get(&public_key)
                        else {
                            for log_id in log_ids {
                                let messages =
                                    remote_needs(&self.store, &log_id, &public_key, 0).await?;
                                sink.send_all(&mut stream::iter(messages.into_iter().map(Ok)))
                                    .await?;
                            }
                            continue;
                        };

                        for log_id in log_ids {
                            let latest_operation = self
                                .store
                                .latest_operation(&public_key, &log_id)
                                .await
                                .expect("memory store error");

                            let log_height = match latest_operation {
                                Some(operation) => operation.header.seq_num,
                                None => continue,
                            };

                            let Some((_, remote_log_height)) = remote_log_heights
                                .iter()
                                .find(|(id, _seq_num)| *id == log_id)
                            else {
                                let messages =
                                    remote_needs(&self.store, &log_id, &public_key, 0).await?;
                                sink.send_all(&mut stream::iter(messages.into_iter().map(Ok)))
                                    .await?;
                                continue;
                            };

                            // Compare log heights sent by the remote with our local logs, if
                            // our logs are more advanced calculate and send operations the
                            // remote is missing.
                            if *remote_log_height < log_height {
                                let messages = remote_needs(
                                    &self.store,
                                    &log_id,
                                    &public_key,
                                    *remote_log_height + 1,
                                )
                                .await?;
                                sink.send_all(&mut stream::iter(messages.into_iter().map(Ok)))
                                    .await?;
                            };
                        }
                    }

                    // As we have processed the remotes `Have` message then we are "done" from
                    // this end.
                    sink.send(Message::SyncDone).await?;
                    sync_done_sent = true;
                }
                Message::Operation(_, _) => {
                    return Err(SyncError::Protocol(
                        "unexpected operation received".to_string(),
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
async fn remote_needs<L, E>(
    store: &impl LogStore<L, E>,
    log_id: &L,
    public_key: &PublicKey,
    from: SeqNum,
) -> Result<Vec<Message<L>>, SyncError>
where
    E: Clone + Serialize,
{
    let mut log = store
        .get_log(public_key, log_id)
        .await
        .map_err(|e| SyncError::Protocol(e.to_string()))?;

    let messages: Vec<Message<L>> = log
        .split_off(from as usize)
        .into_iter()
        .map(|operation| {
            Message::Operation(
                operation.header.to_bytes(),
                operation.body.map(|body| body.to_bytes()),
            )
        })
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
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore};
    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tokio_util::sync::PollSender;

    use crate::{FromSync, SyncProtocol, TopicMap};

    use super::{LogSyncProtocol, Logs, Message};

    #[derive(Clone, Debug)]
    struct LogHeightTopicMap(HashMap<String, Logs<u64>>);

    impl LogHeightTopicMap {
        pub fn new() -> Self {
            LogHeightTopicMap(HashMap::new())
        }

        fn insert(&mut self, topic: &str, scope: Logs<u64>) -> Option<Logs<u64>> {
            self.0.insert(topic.to_string(), scope)
        }
    }

    #[async_trait]
    impl TopicMap<String, Logs<u64>> for LogHeightTopicMap {
        async fn get(&self, topic: &String) -> Option<Logs<u64>> {
            self.0.get(topic).cloned()
        }
    }

    fn generate_operation<E: Clone + Serialize>(
        private_key: &PrivateKey,
        body: Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
        extensions: Option<E>,
    ) -> Operation<E> {
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

        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }

    async fn assert_message_bytes<T>(mut rx: ReadHalf<DuplexStream>, messages: Vec<Message<T>>)
    where
        T: Serialize,
    {
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

    fn to_bytes<T>(messages: Vec<Message<T>>) -> Vec<u8>
    where
        T: Serialize,
    {
        messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        })
    }

    #[tokio::test]
    async fn sync_no_operations_accept() {
        let topic = "messages".to_string();
        let logs = HashMap::new();
        let store = MemoryStore::<u64, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a_stream);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b_stream);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = to_bytes(vec![
            Message::Have(topic.clone(), vec![]),
            Message::SyncDone,
        ]);
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .accept(
                Box::new(&mut peer_a_write.compat_write()),
                Box::new(&mut peer_a_read.compat()),
                Box::new(&mut sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(peer_b_read, vec![Message::<u64>::SyncDone]).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![FromSync::Topic(topic)])
    }

    #[tokio::test]
    async fn sync_no_operations_open() {
        let topic = "messages".to_string();
        let logs = HashMap::new();
        let store = MemoryStore::<u64, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a_stream, peer_b_stream) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a_stream);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b_stream);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = vec![Message::<String>::SyncDone.to_bytes()].concat();
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
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
            vec![Message::Have(topic.clone(), vec![]), Message::SyncDone],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![FromSync::Topic(topic)])
    }

    #[tokio::test]
    async fn sync_operations_accept() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = "messages".to_string();
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        // Setup store with 3 operations in it
        let mut store = MemoryStore::<u64, DefaultExtensions>::new();

        let body = Body::new("Hello, Sloth!".as_bytes());
        let operation0 = generate_operation(&private_key, body.clone(), 0, 0, None, None);
        let operation1 = generate_operation(
            &private_key,
            body.clone(),
            1,
            100,
            Some(operation0.hash),
            None,
        );
        let operation2 = generate_operation(
            &private_key,
            body.clone(),
            2,
            200,
            Some(operation1.hash),
            None,
        );

        // Insert these operations to the store using `TOPIC_ID` as the log id
        store.insert_operation(&operation0, &log_id).await.unwrap();
        store.insert_operation(&operation1, &log_id).await.unwrap();
        store.insert_operation(&operation2, &log_id).await.unwrap();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let messages = vec![
            Message::Have::<String, u64>(topic.clone(), vec![]),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);

        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
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
            Message::Operation(
                operation0.header.to_bytes(),
                operation0.body.map(|body| body.to_bytes()),
            ),
            Message::Operation(
                operation1.header.to_bytes(),
                operation1.body.map(|body| body.to_bytes()),
            ),
            Message::Operation(
                operation2.header.to_bytes(),
                operation2.body.map(|body| body.to_bytes()),
            ),
            Message::SyncDone,
        ];
        assert_message_bytes(peer_b_read, messages).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, [FromSync::Topic(topic)])
    }

    #[tokio::test]
    async fn sync_operations_open() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = "messages".to_string();
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
        let operation0: Operation<DefaultExtensions> =
            generate_operation(&private_key, body.clone(), 0, 0, None, None);
        let operation1: Operation<DefaultExtensions> = generate_operation(
            &private_key,
            body.clone(),
            1,
            100,
            Some(operation0.hash),
            None,
        );
        let operation2: Operation<DefaultExtensions> = generate_operation(
            &private_key,
            body.clone(),
            2,
            200,
            Some(operation1.hash),
            None,
        );

        // Write some message into peer_b's send buffer
        let messages: Vec<Message<u64>> = vec![
            Message::Operation(
                operation0.header.to_bytes(),
                operation0.body.clone().map(|body| body.to_bytes()),
            ),
            Message::Operation(
                operation1.header.to_bytes(),
                operation1.body.clone().map(|body| body.to_bytes()),
            ),
            Message::Operation(
                operation2.header.to_bytes(),
                operation2.body.clone().map(|body| body.to_bytes()),
            ),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let protocol = Arc::new(LogSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
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
                FromSync::Topic(topic),
                FromSync::Data(
                    operation0.header.to_bytes(),
                    operation0.body.map(|body| body.to_bytes()),
                ),
                FromSync::Data(
                    operation1.header.to_bytes(),
                    operation1.body.map(|body| body.to_bytes()),
                ),
                FromSync::Data(
                    operation2.header.to_bytes(),
                    operation2.body.map(|body| body.to_bytes()),
                ),
            ]
        );
    }

    #[tokio::test]
    async fn e2e_sync() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = "messages".to_string();
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        // Create an empty store for peer a
        let store1 = MemoryStore::default();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let peer_a_protocol = Arc::new(LogSyncProtocol {
            topic_map: topic_map.clone(),
            store: store1,
        });

        // Create a store for peer b and populate it with 3 operations
        let mut store2 = MemoryStore::default();
        let body = Body::new("Hello, Sloth!".as_bytes());
        let operation0 = generate_operation(&private_key, body.clone(), 0, 0, None, None);
        let operation1 = generate_operation(
            &private_key,
            body.clone(),
            1,
            100,
            Some(operation0.hash),
            None,
        );
        let operation2 = generate_operation(
            &private_key,
            body.clone(),
            2,
            200,
            Some(operation1.hash),
            None,
        );

        // Insert these operations to the store using `TOPIC_ID` as the log id
        store2.insert_operation(&operation0, &log_id).await.unwrap();
        store2.insert_operation(&operation1, &log_id).await.unwrap();
        store2.insert_operation(&operation2, &log_id).await.unwrap();

        // Construct b log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogSyncProtocol {
            topic_map,
            store: store2,
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink = PollSender::new(peer_a_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let topic_clone = topic.clone();
        let handle1 = tokio::spawn(async move {
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
        let mut sink = PollSender::new(peer_b_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle2 = tokio::spawn(async move {
            peer_b_protocol_clone
                .accept(
                    Box::new(&mut peer_b_write.compat_write()),
                    Box::new(&mut peer_b_read.compat()),
                    Box::new(&mut sink),
                )
                .await
                .unwrap();
        });

        // Wait on both to complete
        let (_, _) = tokio::join!(handle1, handle2);

        let peer_a_expected_messages = vec![
            FromSync::Topic(topic.clone()),
            FromSync::Data(
                operation0.header.to_bytes(),
                operation0.body.map(|body| body.to_bytes()),
            ),
            FromSync::Data(
                operation1.header.to_bytes(),
                operation1.body.map(|body| body.to_bytes()),
            ),
            FromSync::Data(
                operation2.header.to_bytes(),
                operation2.body.map(|body| body.to_bytes()),
            ),
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;

        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![FromSync::Topic(topic.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;

        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }

    #[tokio::test]
    async fn e2e_partial_sync() {
        let private_key = PrivateKey::new();
        let log_id = 0;
        let topic = "messages".to_string();
        let logs = HashMap::from([(private_key.public_key(), vec![log_id])]);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let operation0 = generate_operation(&private_key, body.clone(), 0, 0, None, None);
        let operation1 = generate_operation(
            &private_key,
            body.clone(),
            1,
            100,
            Some(operation0.hash),
            None,
        );
        let operation2 = generate_operation(
            &private_key,
            body.clone(),
            2,
            200,
            Some(operation1.hash),
            None,
        );

        // Create a store for peer a and populate it with one operation
        let mut store1 = MemoryStore::default();
        store1.insert_operation(&operation0, &0).await.unwrap();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogHeightTopicMap::new();
        topic_map.insert(&topic, logs);
        let peer_a_protocol = Arc::new(LogSyncProtocol {
            topic_map: topic_map.clone(),
            store: store1,
        });

        // Create a store for peer b and populate it with 3 operations.
        let mut store2 = MemoryStore::default();

        // Insert these operations to the store using `TOPIC_ID` as the log id
        store2.insert_operation(&operation0, &log_id).await.unwrap();
        store2.insert_operation(&operation1, &log_id).await.unwrap();
        store2.insert_operation(&operation2, &log_id).await.unwrap();

        // Construct a log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogSyncProtocol {
            topic_map,
            store: store2,
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink = PollSender::new(peer_a_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let topic_clone = topic.clone();
        let handle1 = tokio::spawn(async move {
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
        let mut sink = PollSender::new(peer_b_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle2 = tokio::spawn(async move {
            peer_b_protocol_clone
                .accept(
                    Box::new(&mut peer_b_write.compat_write()),
                    Box::new(&mut peer_b_read.compat()),
                    Box::new(&mut sink),
                )
                .await
                .unwrap();
        });

        // Wait on both to complete
        let (_, _) = tokio::join!(handle1, handle2);

        let peer_a_expected_messages = vec![
            FromSync::Topic(topic.clone()),
            FromSync::Data(
                operation1.header.to_bytes(),
                operation1.body.map(|body| body.to_bytes()),
            ),
            FromSync::Data(
                operation2.header.to_bytes(),
                operation2.body.map(|body| body.to_bytes()),
            ),
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;

        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![FromSync::Topic(topic.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;

        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }
}
