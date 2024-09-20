// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use p2panda_core::PublicKey;
use p2panda_store::{LogStore, MemoryStore, TopicMap};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::protocols::utils::{into_sink, into_stream};
use crate::traits::SyncProtocol;
use crate::{FromSync, SyncError, TopicId};

type SeqNum = u64;
pub type LogHeights = Vec<(PublicKey, SeqNum)>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Message<T = String> {
    Have(TopicId, T, LogHeights),
    RawOperation(Vec<u8>),
    SyncDone,
}

#[cfg(test)]
impl<T> Message<T>
where
    T: Serialize,
{
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(&self, &mut bytes).expect("type can be serialized");
        bytes
    }
}

static LOG_HEIGHT_PROTOCOL_NAME: &str = "p2panda/log_height";

#[derive(Clone, Debug)]
pub struct LogHeightSyncProtocol<S, T, E> {
    pub topic_map: S,
    pub store: MemoryStore<T, E>,
}

#[async_trait]
impl<'a, S, T, E> SyncProtocol<'a> for LogHeightSyncProtocol<S, T, E>
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
        LOG_HEIGHT_PROTOCOL_NAME
    }

    #[allow(unused_assignments)]
    async fn open(
        self: Arc<Self>,
        topic: &TopicId,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_sink(tx);
        let mut stream = into_stream(rx);

        let Some(log_id) = self.topic_map.get(topic) else {
            return Err(SyncError::Protocol("Unknown topic id".to_string()));
        };
        let local_log_heights = self
            .store
            .get_log_heights(&log_id)
            .await
            .expect("memory store error");

        sink.send(Message::<T>::Have(
            *topic,
            log_id.clone(),
            local_log_heights.clone(),
        ))
        .await?;
        // As we initiated this sync session we are done after sending the Have message.
        sink.send(Message::SyncDone).await?;
        sync_done_sent = true;

        app_tx.send(FromSync::Topic(*topic)).await?;

        while let Some(result) = stream.next().await {
            let message: Message<T> = result?;
            debug!("message received: {:?}", message);

            match message {
                Message::Have(_, _, _) => {
                    return Err(SyncError::Protocol(
                        "unexpected Have message received".to_string(),
                    ))
                }
                Message::RawOperation(bytes) => {
                    app_tx.send(FromSync::Bytes(bytes)).await?;
                }
                Message::SyncDone => {
                    sync_done_received = true;
                }
            };

            if sync_done_received && sync_done_sent {
                break;
            }
        }

        // @NOTE: It's important to call this method before the streams are dropped, it makes
        // sure all bytes are flushed from the sink before closing so that no messages are
        // lost.
        sink.flush().await?;
        sink.close().await?;

        app_tx.flush().await?;
        app_tx.close().await?;
        debug!("sync session finished");

        Ok(())
    }

    #[allow(unused_assignments)]
    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_sink(tx);
        let mut stream = into_stream(rx);

        while let Some(result) = stream.next().await {
            let message: Message<T> = result?;
            debug!("message received: {:?}", message);

            let replies = match &message {
                Message::Have(topic, log_id, log_heights) => {
                    app_tx.send(FromSync::Topic(*topic)).await?;
                    let mut messages: Vec<Message<T>> = vec![];

                    let local_log_heights = self
                        .store
                        .get_log_heights(log_id)
                        .await
                        .expect("memory store error");

                    for (public_key, seq_num) in local_log_heights {
                        let mut remote_needs = vec![];

                        for (remote_pub_key, remote_seq_num) in log_heights.iter() {
                            // For logs where both peers know of the author, compare seq numbers
                            // and if ours is higher then we know the peer needs to be sent the
                            // newer operations we have.
                            if *remote_pub_key == public_key && *remote_seq_num < seq_num {
                                remote_needs.push((public_key, *remote_seq_num + 1));
                                continue;
                            };
                        }

                        // If the author is not known by both peers, then see if _we_ are the
                        // ones who know of log the remote needs.
                        if !log_heights
                            .iter()
                            .any(|(remote_public_key, _)| public_key == *remote_public_key)
                        {
                            remote_needs.push((public_key, 0));
                        }

                        // For every log the remote needs send only the operations they are missing.
                        for (public_key, seq_num) in remote_needs {
                            let mut log = self
                                .store
                                .get_log(&public_key, log_id)
                                .await
                                .map_err(|e| SyncError::Protocol(e.to_string()))?;
                            log.split_off(seq_num as usize)
                                .into_iter()
                                .for_each(|operation| {
                                    let mut bytes = Vec::new();
                                    ciborium::into_writer(
                                        &(operation.header, operation.body),
                                        &mut bytes,
                                    )
                                    .expect("invalid operation found in store");

                                    messages.push(Message::RawOperation(bytes))
                                });
                        }
                    }

                    // As we have processed the remotes `Have` message then we are "done" from
                    // this end.
                    messages.push(Message::SyncDone);
                    sync_done_sent = true;

                    messages
                }
                Message::RawOperation(_) => {
                    return Err(SyncError::Protocol(
                        "unexpected operation received".to_string(),
                    ));
                }
                Message::SyncDone => {
                    sync_done_received = true;
                    vec![]
                }
            };

            // @TODO: we'd rather process all messages at once using `send_all`. For this
            // we need to turn `replies` into a stream.
            for message in replies {
                sink.send(message).await?;
            }

            if sync_done_received && sync_done_sent {
                break;
            }
        }

        // @NOTE: It's important to call this method before the streams are dropped, it makes
        // sure all bytes are flushed from the sink before closing so that no messages are
        // lost.
        sink.flush().await?;
        sink.close().await?;

        app_tx.flush().await?;
        app_tx.close().await?;
        debug!("sync session finished");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use futures::SinkExt;
    use p2panda_core::extensions::DefaultExtensions;
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore};
    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tokio_util::sync::PollSender;

    use crate::traits::SyncProtocol;
    use crate::{FromSync, TopicId};

    use super::{LogHeightSyncProtocol, Message, TopicMap};

    #[derive(Clone, Debug)]
    struct LogIdTopicMap(HashMap<TopicId, String>);

    impl LogIdTopicMap {
        pub fn new() -> Self {
            LogIdTopicMap(HashMap::new())
        }

        fn insert(&mut self, topic: TopicId, scope: String) -> Option<String> {
            self.0.insert(topic, scope)
        }
    }

    impl TopicMap<TopicId, String> for LogIdTopicMap {
        fn get(&self, topic: &TopicId) -> Option<String> {
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
            Message::Have(TOPIC_ID.clone(), log_id.clone(), vec![]),
            Message::SyncDone,
        ]);
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, log_id.clone());
        let protocol = Arc::new(LogHeightSyncProtocol { topic_map, store });
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
        topic_map.insert(TOPIC_ID, log_id.clone());
        let protocol = Arc::new(LogHeightSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .open(
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
                Message::Have(TOPIC_ID.clone(), log_id.clone(), vec![]),
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

        // Setup store with 3 operations in it
        let mut store = MemoryStore::<String, DefaultExtensions>::new();
        let private_key = PrivateKey::new();

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
        let messages: Vec<Message<String>> = vec![
            Message::Have(TOPIC_ID.clone(), log_id.clone(), vec![]),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, log_id.clone());
        let protocol = Arc::new(LogHeightSyncProtocol { topic_map, store });
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
        let mut operation0_bytes = Vec::new();
        ciborium::into_writer(&(operation0.header, operation0.body), &mut operation0_bytes)
            .unwrap();
        let mut operation1_bytes = Vec::new();
        ciborium::into_writer(&(operation1.header, operation1.body), &mut operation1_bytes)
            .unwrap();
        let mut operation2_bytes = Vec::new();
        ciborium::into_writer(&(operation2.header, operation2.body), &mut operation2_bytes)
            .unwrap();

        let messages: Vec<Message<String>> = vec![
            Message::RawOperation(operation0_bytes),
            Message::RawOperation(operation1_bytes),
            Message::RawOperation(operation2_bytes),
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
        let mut operation0_bytes = Vec::new();
        ciborium::into_writer(&(operation0.header, operation0.body), &mut operation0_bytes)
            .unwrap();
        let mut operation1_bytes = Vec::new();
        ciborium::into_writer(&(operation1.header, operation1.body), &mut operation1_bytes)
            .unwrap();
        let mut operation2_bytes = Vec::new();
        ciborium::into_writer(&(operation2.header, operation2.body), &mut operation2_bytes)
            .unwrap();
        let messages: Vec<Message<String>> = vec![
            Message::RawOperation(operation0_bytes.clone()),
            Message::RawOperation(operation1_bytes.clone()),
            Message::RawOperation(operation2_bytes.clone()),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, log_id.clone());
        let protocol = Arc::new(LogHeightSyncProtocol { topic_map, store });
        let mut sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .open(
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
                Message::Have(TOPIC_ID.clone(), log_id.clone(), vec![]),
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
                FromSync::Bytes(operation0_bytes),
                FromSync::Bytes(operation1_bytes),
                FromSync::Bytes(operation2_bytes)
            ]
        );
    }

    #[tokio::test]
    async fn e2e_sync() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        let log_id = String::from("messages");

        // Create an empty store for peer a
        let store1 = MemoryStore::default();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, log_id.clone());
        let peer_a_protocol = Arc::new(LogHeightSyncProtocol {
            topic_map: topic_map.clone(),
            store: store1,
        });

        // Create a store for peer b and populate it with 3 operations
        let mut store2 = MemoryStore::default();
        let private_key = PrivateKey::new();
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
        let peer_b_protocol = Arc::new(LogHeightSyncProtocol {
            topic_map,
            store: store2,
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink = PollSender::new(peer_a_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .open(
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

        let mut operation0_bytes = Vec::new();
        ciborium::into_writer(&(operation0.header, operation0.body), &mut operation0_bytes)
            .unwrap();
        let mut operation1_bytes = Vec::new();
        ciborium::into_writer(&(operation1.header, operation1.body), &mut operation1_bytes)
            .unwrap();
        let mut operation2_bytes = Vec::new();
        ciborium::into_writer(&(operation2.header, operation2.body), &mut operation2_bytes)
            .unwrap();

        let peer_a_expected_messages = vec![
            FromSync::Topic(TOPIC_ID.clone()),
            FromSync::Bytes(operation0_bytes),
            FromSync::Bytes(operation1_bytes),
            FromSync::Bytes(operation2_bytes),
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
        store1.insert_operation(&operation0, &log_id).await.unwrap();

        // Construct a log height protocol and engine for peer a
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, log_id.clone());
        let peer_a_protocol = Arc::new(LogHeightSyncProtocol {
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
        let peer_b_protocol = Arc::new(LogHeightSyncProtocol {
            topic_map,
            store: store2,
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let mut sink = PollSender::new(peer_a_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .open(
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

        let mut operation1_bytes = Vec::new();
        ciborium::into_writer(&(operation1.header, operation1.body), &mut operation1_bytes)
            .unwrap();
        let mut operation2_bytes = Vec::new();
        ciborium::into_writer(&(operation2.header, operation2.body), &mut operation2_bytes)
            .unwrap();

        let peer_a_expected_messages = vec![
            FromSync::Topic(TOPIC_ID.clone()),
            FromSync::Bytes(operation1_bytes),
            FromSync::Bytes(operation2_bytes),
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
