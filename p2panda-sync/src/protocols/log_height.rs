// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink, SinkExt, StreamExt};
use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Body, Header, Operation, PublicKey};
use p2panda_store::{LogStore, MemoryStore, OperationStore};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::protocols::utils::{into_sink, into_stream};
use crate::traits::{AppMessage, SyncProtocol};
use crate::{SyncError, TopicId};

type LogId = String;
type SeqNum = u64;
pub type LogHeights = Vec<(PublicKey, SeqNum)>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Message<E = DefaultExtensions> {
    Have(LogId, LogHeights),
    Operation(Header<E>, Option<Body>),
    SyncDone,
}

#[cfg(test)]
impl<E> Message<E>
where
    E: Serialize,
{
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(&self, &mut bytes).expect("type can be serialized");
        bytes
    }
}

static LOG_HEIGHT_PROTOCOL_NAME: &str = "p2panda/log_height";

#[derive(Clone, Debug, Default)]
pub struct LogHeightSyncProtocol<E>
where
    E: Clone + Default,
    MemoryStore<LogId, E>: Default,
{
    pub log_ids: HashMap<TopicId, LogId>,
    pub store: Arc<RwLock<MemoryStore<LogId, E>>>,
}

impl<E> LogHeightSyncProtocol<E>
where
    E: Clone + Default,
    MemoryStore<LogId, E>: Default,
{
    pub fn log_id(&self, topic: &TopicId) -> Option<&LogId> {
        self.log_ids.get(topic)
    }
    pub fn read_store(&self) -> RwLockReadGuard<MemoryStore<LogId, E>> {
        self.store.read().expect("error getting read lock on store")
    }

    pub fn write_store(&self) -> RwLockWriteGuard<MemoryStore<LogId, E>> {
        self.store
            .write()
            .expect("error getting write lock on store")
    }
}

#[async_trait]
impl<E> SyncProtocol for LogHeightSyncProtocol<E>
where
    E: Clone + Debug + Default + Send + Sync + for<'a> Deserialize<'a> + Serialize,
    MemoryStore<LogId, E>: Default,
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
            .get_log_heights(log_id.to_string())
            .expect("memory store error");

        sink.send(Message::<E>::Have(
            log_id.to_string(),
            local_log_heights.clone(),
        ))
        .await?;
        // As we initiated this sync session we are done after sending the Have message.
        sink.send(Message::SyncDone).await?;
        sync_done_sent = true;

        while let Some(result) = stream.next().await {
            let message = result?;
            debug!("message received: {:?}", message);

            match &message {
                Message::Have(_, _) => {
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
                    let inserted = self
                        .write_store()
                        .insert_operation(operation.clone(), log_id.to_string())
                        .map_err(|e| SyncError::Protocol(e.to_string()))?;

                    if inserted {
                        let mut bytes = Vec::new();
                        ciborium::into_writer(&(operation.header, operation.body), &mut bytes)
                            .map_err(|e| SyncError::Protocol(e.to_string()))?;
                        app_tx.send(AppMessage::Bytes(bytes)).await?;
                    }
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
        debug!("sync session finished");

        Ok(())
    }

    #[allow(unused_assignments)]
    async fn accept(
        self: Arc<Self>,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        _app_tx: Box<dyn Sink<AppMessage, Error = SyncError> + Send + Unpin>,
    ) -> Result<(), SyncError> {
        let mut sync_done_sent = false;
        let mut sync_done_received = false;

        let mut sink = into_sink(tx);
        let mut stream = into_stream(rx);

        while let Some(result) = stream.next().await {
            let message: Message<E> = result?;
            debug!("message received: {:?}", message);

            let mut replies = match &message {
                Message::Have(log_id, log_heights) => {
                    let mut messages = vec![];

                    let local_log_heights = self
                        .read_store()
                        .get_log_heights(log_id.to_string())
                        .expect("memory store error");

                    for (public_key, seq_num) in local_log_heights {
                        let mut remote_needs = vec![];

                        for (remote_pub_key, remote_seq_num) in log_heights.iter() {
                            // For logs where both peers know of the author, compare seq numbers
                            // and if ours is higher then we know the peer needs to be sent the
                            // newer operations we have.
                            if *remote_pub_key == public_key && *remote_seq_num < seq_num {
                                remote_needs.push((public_key, *remote_seq_num));
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
                                .get_log(public_key, log_id.to_string())
                                .map_err(|e| SyncError::Protocol(e.to_string()))?;
                            log.split_off(seq_num as usize + 1)
                                .into_iter()
                                .for_each(|operation| {
                                    messages
                                        .push(Message::Operation(operation.header, operation.body))
                                });
                        }
                    }

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
            // As we have processed the remotes `Have` message then we are "done" from
            // this end.
            replies.push(Message::SyncDone);
            sync_done_sent = true;

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
    use p2panda_store::{LogStore, MemoryStore, OperationStore};
    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    use tokio_util::sync::PollSender;

    use crate::traits::SyncProtocol;

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

    #[tokio::test]
    async fn run_sync_strategy() {
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

        // Create a duplex stream which simulate both ends of a bi-directional network connection
        let (peer_a, mut peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);

        // Write some message into peer_b's send buffer
        let message1: Message<DefaultExtensions> =
            Message::Have(LOG_ID.to_string(), vec![(private_key.public_key(), 0)]);
        let message2: Message<DefaultExtensions> = Message::SyncDone;
        let message_bytes = vec![message1.to_bytes(), message2.to_bytes()].concat();
        peer_b.write_all(&message_bytes[..]).await.unwrap();

        // Run the sync session (which consumes the above messages)
        let protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store)),
        });
        let (app_tx, app_rx) = mpsc::channel(128);
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
        let handle = tokio::spawn(async move {
            let _ = protocol
                .accept(
                    Box::new(peer_a_write.compat_write()),
                    Box::new(peer_a_read.compat()),
                    Box::new(sink),
                )
                .await
                .unwrap();
        });
        handle.await.unwrap();

        // Read the entire buffer out of peer_b's read stream
        let mut buf = Vec::new();
        peer_b.read_to_end(&mut buf).await.unwrap();

        let received_message1 =
            Message::Operation(operation1.header.clone(), operation1.body.clone());
        let received_message2 =
            Message::Operation(operation2.header.clone(), operation2.body.clone());
        let receive_message3 = Message::<DefaultExtensions>::SyncDone;
        assert_eq!(
            buf,
            [
                received_message1.to_bytes(),
                received_message2.to_bytes(),
                receive_message3.to_bytes()
            ]
            .concat()
        );
    }

    #[tokio::test]
    async fn sync_operation_store() {
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        const LOG_ID: &str = "messages";

        // Create a store for peer a and populate it with operations
        let store1 = MemoryStore::default();

        // Construct a log height protocol and engine for peer a
        let peer_a_protocol = Arc::new(LogHeightSyncProtocol {
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
            store: Arc::new(RwLock::new(store1)),
        });

        // Create an empty store for peer a and construct their sync protocol and engine
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

        // Construct a log height protocol and engine for peer a
        let peer_b_protocol = Arc::new(LogHeightSyncProtocol {
            store: Arc::new(RwLock::new(store2)),
            log_ids: HashMap::from([(TOPIC_ID, LOG_ID.to_string())]),
        });

        // Create a duplex stream which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        let peer_a_protocol_clone = peer_a_protocol.clone();
        let (app_tx, app_rx) = mpsc::channel(128);
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
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

        let peer_b_protocol_clone = peer_b_protocol.clone();
        let (app_tx, app_rx) = mpsc::channel(128);
        let sink =
            PollSender::new(app_tx).sink_map_err(|e| crate::SyncError::Protocol(e.to_string()));
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

        // Check log heights are now equal
        let peer_a_log_heights = peer_a_protocol
            .read_store()
            .get_log_heights(LOG_ID.to_string())
            .unwrap();

        let peer_b_log_heights = peer_b_protocol
            .read_store()
            .get_log_heights(LOG_ID.to_string())
            .unwrap();

        assert_eq!(peer_a_log_heights, peer_b_log_heights);
    }
}
