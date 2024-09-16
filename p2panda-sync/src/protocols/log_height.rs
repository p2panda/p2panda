// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Body, Header, Operation, PublicKey};
use p2panda_store::{LogStore, MemoryStore};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::protocols::utils::{into_sink, into_stream};
use crate::traits::{AppMessage, SyncProtocol};
use crate::{SyncError, TopicId};

type SeqNum = u64;
pub type LogHeights = Vec<(PublicKey, SeqNum)>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Message<T = String, E = DefaultExtensions> {
    Have(TopicId, T, LogHeights),
    Operation(Header<E>, Option<Body>),
    SyncDone,
}

#[cfg(test)]
impl<T, E> Message<T, E>
where
    T: Serialize,
    E: Serialize,
{
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(&self, &mut bytes).expect("type can be serialized");
        bytes
    }
}

static LOG_HEIGHT_PROTOCOL_NAME: &str = "p2panda/log_height";

#[derive(Clone, Debug)]
pub struct LogHeightSyncProtocol<T, E> {
    pub log_ids: HashMap<TopicId, T>,
    pub store: Arc<RwLock<MemoryStore<T, E>>>,
}

impl<T, E> LogHeightSyncProtocol<T, E>
where
    T: Clone + Debug + Default,
    E: Clone + Default,
{
    pub fn log_id(&self, topic: &TopicId) -> Option<&T> {
        self.log_ids.get(topic)
    }
    pub fn read_store(&self) -> RwLockReadGuard<MemoryStore<T, E>> {
        self.store.read().expect("error getting read lock on store")
    }

    pub fn write_store(&self) -> RwLockWriteGuard<MemoryStore<T, E>> {
        self.store
            .write()
            .expect("error getting write lock on store")
    }
}

#[async_trait]
impl<T, E> SyncProtocol for LogHeightSyncProtocol<T, E>
where
    T: Clone + Debug + Default + Eq + Hash + Send + Sync + for<'a> Deserialize<'a> + Serialize,
    E: Clone + Debug + Default + Send + Sync + for<'a> Deserialize<'a> + Serialize,
{
    fn name(&self) -> &'static str {
        LOG_HEIGHT_PROTOCOL_NAME
    }

    #[allow(unused_assignments)]
    async fn open(
        self: Arc<Self>,
        topic: &TopicId,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        mut app_tx: Box<dyn Sink<AppMessage, Error = SyncError> + Send + Unpin>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_sink(tx);
        let mut stream = into_stream(rx);

        let Some(log_id) = self.log_id(topic) else {
            return Err(SyncError::Protocol("Unknown topic id".to_string()));
        };
        let local_log_heights = self
            .read_store()
            .get_log_heights(log_id.to_owned())
            .expect("memory store error");

        sink.send(Message::<T, E>::Have(
            *topic,
            log_id.clone(),
            local_log_heights.clone(),
        ))
        .await?;
        // As we initiated this sync session we are done after sending the Have message.
        sink.send(Message::SyncDone).await?;
        sync_done_sent = true;

        app_tx.send(AppMessage::Topic(*topic)).await?;

        while let Some(result) = stream.next().await {
            let message: Message<T, E> = result?;
            debug!("message received: {:?}", message);

            match &message {
                Message::Have(_, _, _) => {
                    return Err(SyncError::Protocol(
                        "unexpected Have message received".to_string(),
                    ))
                }
                Message::Operation(header, body) => {
                    let operation = Operation {
                        hash: header.hash(),
                        header: header.clone(),
                        body: body.clone(),
                    };
                    let mut bytes = Vec::new();
                    ciborium::into_writer(&(operation.header, operation.body), &mut bytes)
                        .map_err(|e| SyncError::Protocol(e.to_string()))?;
                    app_tx.send(AppMessage::Bytes(bytes)).await?;
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
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        mut app_tx: Box<dyn Sink<AppMessage, Error = SyncError> + Send + Unpin>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_sink(tx);
        let mut stream = into_stream(rx);

        while let Some(result) = stream.next().await {
            let message: Message<T, E> = result?;
            debug!("message received: {:?}", message);

            let replies = match &message {
                Message::Have(topic, log_id, log_heights) => {
                    app_tx.send(AppMessage::Topic(*topic)).await?;
                    let mut messages: Vec<Message<T, E>> = vec![];

                    let local_log_heights = self
                        .read_store()
                        .get_log_heights(log_id.to_owned())
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
                                .read_store()
                                .get_log(public_key, log_id.to_owned())
                                .map_err(|e| SyncError::Protocol(e.to_string()))?;
                            log.split_off(seq_num as usize)
                                .into_iter()
                                .for_each(|operation| {
                                    messages
                                        .push(Message::Operation(operation.header, operation.body))
                                });
                        }
                    }

                    // As we have processed the remotes `Have` message then we are "done" from
                    // this end.
                    messages.push(Message::SyncDone);
                    sync_done_sent = true;

                    messages
                }
                Message::Operation(_, _) => {
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
    use std::sync::{Arc, RwLock};

    use futures::SinkExt;
    use p2panda_core::extensions::DefaultExtensions;
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore};
    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tokio_util::sync::PollSender;

    use crate::traits::{AppMessage, SyncProtocol};

    use super::{LogHeightSyncProtocol, Message};

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

    fn to_bytes(messages: Vec<Message<String, DefaultExtensions>>) -> Vec<u8> {
        messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        })
    }

    #[tokio::test]
    async fn sync_no_operations_accept() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";

        let store = MemoryStore::<String, DefaultExtensions>::new();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let message_bytes = to_bytes(vec![
            Message::Have(TOPIC_ID.clone(), LOG_ID.to_string(), vec![]),
            Message::SyncDone,
        ]);
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store)),
        });
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .accept(
                Box::new(peer_a_write.compat_write()),
                Box::new(peer_a_read.compat()),
                Box::new(sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(peer_b_read, vec![Message::SyncDone]).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![AppMessage::Topic(TOPIC_ID)])
    }

    #[tokio::test]
    async fn sync_no_operations_open() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";

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
        let protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store)),
        });
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .open(
                &TOPIC_ID,
                Box::new(peer_a_write.compat_write()),
                Box::new(peer_a_read.compat()),
                Box::new(sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(
            peer_b_read,
            vec![
                Message::Have(TOPIC_ID.clone(), LOG_ID.to_string(), vec![]),
                Message::SyncDone,
            ],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, vec![AppMessage::Topic(TOPIC_ID)])
    }

    #[tokio::test]
    async fn sync_operations_accept() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";

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
        store
            .insert_operation(operation0.clone(), LOG_ID.to_string())
            .unwrap();
        store
            .insert_operation(operation1.clone(), LOG_ID.to_string())
            .unwrap();
        store
            .insert_operation(operation2.clone(), LOG_ID.to_string())
            .unwrap();

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, mut peer_b_write) = tokio::io::split(peer_b);

        // Channel for sending messages out of a running sync session
        let (app_tx, mut app_rx) = mpsc::channel(128);

        // Write some message into peer_b's send buffer
        let messages: Vec<Message<String, DefaultExtensions>> = vec![
            Message::Have(TOPIC_ID.clone(), LOG_ID.to_string(), vec![]),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Accept a sync session on peer a (which consumes the above messages)
        let protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store)),
        });
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .accept(
                Box::new(peer_a_write.compat_write()),
                Box::new(peer_a_read.compat()),
                Box::new(sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        let messages: Vec<Message<String, DefaultExtensions>> = vec![
            Message::Operation(operation0.header.clone(), operation0.body.clone()),
            Message::Operation(operation1.header.clone(), operation1.body.clone()),
            Message::Operation(operation2.header.clone(), operation2.body.clone()),
            Message::SyncDone,
        ];
        assert_message_bytes(peer_b_read, messages).await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(messages, [AppMessage::Topic(TOPIC_ID)])
    }

    #[tokio::test]
    async fn sync_operations_open() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";
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
        let messages: Vec<Message<String, DefaultExtensions>> = vec![
            Message::Operation(operation0.header.clone(), operation0.body.clone()),
            Message::Operation(operation1.header.clone(), operation1.body.clone()),
            Message::Operation(operation2.header.clone(), operation2.body.clone()),
            Message::SyncDone,
        ];
        let message_bytes = messages.iter().fold(Vec::new(), |mut acc, message| {
            acc.extend(message.to_bytes());
            acc
        });
        peer_b_write.write_all(&message_bytes[..]).await.unwrap();

        // Open a sync session on peer a (which consumes the above messages)
        let protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store)),
        });
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let _ = protocol
            .open(
                &TOPIC_ID,
                Box::new(peer_a_write.compat_write()),
                Box::new(peer_a_read.compat()),
                Box::new(sink),
            )
            .await
            .unwrap();

        // Assert that peer a sent peer b the expected messages
        assert_message_bytes(
            peer_b_read,
            vec![
                Message::Have(TOPIC_ID.clone(), LOG_ID.to_string(), vec![]),
                Message::SyncDone,
            ],
        )
        .await;

        // Assert that peer a sent the expected messages on it's app channel
        let mut operation0_bytes = Vec::new();
        ciborium::into_writer(&(operation0.header, operation0.body), &mut operation0_bytes)
            .unwrap();
        let mut operation1_bytes = Vec::new();
        ciborium::into_writer(&(operation1.header, operation1.body), &mut operation1_bytes)
            .unwrap();
        let mut operation2_bytes = Vec::new();
        ciborium::into_writer(&(operation2.header, operation2.body), &mut operation2_bytes)
            .unwrap();

        let mut messages = Vec::new();
        app_rx.recv_many(&mut messages, 10).await;
        assert_eq!(
            messages,
            [
                AppMessage::Topic(TOPIC_ID),
                AppMessage::Bytes(operation0_bytes),
                AppMessage::Bytes(operation1_bytes),
                AppMessage::Bytes(operation2_bytes)
            ]
        );
    }

    #[tokio::test]
    async fn e2e_sync() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";

        // Create an empty store for peer a
        let store1 = MemoryStore::default();

        // Construct a log height protocol and engine for peer a
        let peer_a_protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store1)),
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
        store2
            .insert_operation(operation0.clone(), LOG_ID.to_string())
            .unwrap();
        store2
            .insert_operation(operation1.clone(), LOG_ID.to_string())
            .unwrap();
        store2
            .insert_operation(operation2.clone(), LOG_ID.to_string())
            .unwrap();

        // Construct b log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogHeightSyncProtocol {
            store: Arc::new(RwLock::new(store2)),
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let sink = PollSender::new(peer_a_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .open(
                    &TOPIC_ID,
                    Box::new(peer_a_write.compat_write()),
                    Box::new(peer_a_read.compat()),
                    Box::new(sink),
                )
                .await
                .unwrap();
        });

        // Spawn a task which accepts a sync session on peer b runs it to completion
        let peer_b_protocol_clone = peer_b_protocol.clone();
        let (peer_b_app_tx, mut peer_b_app_rx) = mpsc::channel(128);
        let sink = PollSender::new(peer_b_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle2 = tokio::spawn(async move {
            peer_b_protocol_clone
                .accept(
                    Box::new(peer_b_write.compat_write()),
                    Box::new(peer_b_read.compat()),
                    Box::new(sink),
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
            AppMessage::Topic(TOPIC_ID.clone()),
            AppMessage::Bytes(operation0_bytes),
            AppMessage::Bytes(operation1_bytes),
            AppMessage::Bytes(operation2_bytes),
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;

        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![AppMessage::Topic(TOPIC_ID.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;

        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }

    #[tokio::test]
    async fn e2e_partial_sync() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";
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
        store1
            .insert_operation(operation0.clone(), LOG_ID.to_string())
            .unwrap();

        // Construct a log height protocol and engine for peer a
        let peer_a_protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store1)),
        });

        // Create a store for peer b and populate it with 3 operations.
        let mut store2 = MemoryStore::default();

        // Insert these operations to the store using `TOPIC_ID` as the log id
        store2
            .insert_operation(operation0.clone(), LOG_ID.to_string())
            .unwrap();
        store2
            .insert_operation(operation1.clone(), LOG_ID.to_string())
            .unwrap();
        store2
            .insert_operation(operation2.clone(), LOG_ID.to_string())
            .unwrap();

        // Construct a log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogHeightSyncProtocol {
            store: Arc::new(RwLock::new(store2)),
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
        });

        // Duplex streams which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Spawn a task which opens a sync session from peer a runs it to completion
        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (peer_a_app_tx, mut peer_a_app_rx) = mpsc::channel(128);
        let sink = PollSender::new(peer_a_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle1 = tokio::spawn(async move {
            peer_a_protocol_clone
                .open(
                    &TOPIC_ID,
                    Box::new(peer_a_write.compat_write()),
                    Box::new(peer_a_read.compat()),
                    Box::new(sink),
                )
                .await
                .unwrap();
        });

        // Spawn a task which accepts a sync session on peer b runs it to completion
        let peer_b_protocol_clone = peer_b_protocol.clone();
        let (peer_b_app_tx, mut peer_b_app_rx) = mpsc::channel(128);
        let sink = PollSender::new(peer_b_app_tx)
            .sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle2 = tokio::spawn(async move {
            peer_b_protocol_clone
                .accept(
                    Box::new(peer_b_write.compat_write()),
                    Box::new(peer_b_read.compat()),
                    Box::new(sink),
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
            AppMessage::Topic(TOPIC_ID.clone()),
            AppMessage::Bytes(operation1_bytes),
            AppMessage::Bytes(operation2_bytes),
        ];

        let mut peer_a_messages = Vec::new();
        peer_a_app_rx.recv_many(&mut peer_a_messages, 10).await;

        assert_eq!(peer_a_messages, peer_a_expected_messages);

        let peer_b_expected_messages = vec![AppMessage::Topic(TOPIC_ID.clone())];
        let mut peer_b_messages = Vec::new();
        peer_b_app_rx.recv_many(&mut peer_b_messages, 10).await;

        assert_eq!(peer_b_messages, peer_b_expected_messages);
    }
}
