use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use futures::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Body, Header, Operation, PublicKey};
use p2panda_store::{LogStore, MemoryStore, OperationStore};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::traits::{SyncError, SyncProtocol};

type LogId = String;
type SeqNum = u64;
pub type LogHeights = Vec<(PublicKey, SeqNum)>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Message<E = DefaultExtensions> {
    Have(LogHeights),
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

#[derive(Clone, Debug, Default)]
pub struct LogHeightSyncProtocol {
    pub sync_done_sent: bool,
    pub sync_done_received: bool,
    pub store: Arc<RwLock<MemoryStore<LogId, DefaultExtensions>>>,
}

impl LogHeightSyncProtocol {
    pub fn read_store(&self) -> RwLockReadGuard<MemoryStore<LogId, DefaultExtensions>> {
        self.store.read().expect("error getting read lock on store")
    }

    pub fn write_store(&self) -> RwLockWriteGuard<MemoryStore<LogId, DefaultExtensions>> {
        self.store
            .write()
            .expect("error getting write lock on store")
    }
}

impl SyncProtocol for LogHeightSyncProtocol {
    type Topic = LogId;
    type Message = Message;

    async fn run(
        mut self,
        topic: Self::Topic,
        mut sink: impl Sink<Self::Message, Error = SyncError> + Unpin,
        mut stream: impl Stream<Item = Result<Self::Message, SyncError>> + Unpin,
    ) -> Result<(), SyncError> {
        let local_log_heights = self
            .read_store()
            .get_log_heights(topic.to_string())
            .expect("memory store error");

        sink.send(Message::Have(local_log_heights.clone())).await?;

        while let Some(result) = stream.next().await {
            let message = result?;
            debug!("message received: {:?}", message);

            let replies = match &message {
                Message::Have(log_heights) => {
                    let mut messages = vec![];

                    let local_log_heights = self
                        .read_store()
                        .get_log_heights(topic.to_string())
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
                                .get_log(public_key, topic.to_string())
                                .map_err(|e| SyncError::Protocol(e.to_string()))?;
                            log.split_off(seq_num as usize + 1)
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
                    self.sync_done_sent = true;
                    messages
                }
                Message::Operation(header, body) => {
                    let operation = Operation {
                        hash: header.hash(),
                        header: header.clone(),
                        body: body.clone(),
                    };
                    self.write_store()
                        .insert_operation(operation, topic.to_string())
                        .map_err(|e| SyncError::Protocol(e.to_string()))?;
                    vec![]
                }
                Message::SyncDone => {
                    self.sync_done_received = true;
                    vec![]
                }
            };

            // @TODO: we'd rather process all messages at once using `send_all`. For this
            // we need to turn `replies` into a stream.
            for message in replies {
                sink.send(message).await?;
            }

            if self.sync_done_received && self.sync_done_sent {
                break;
            }
        }

        // @TODO: should we actually need to do this?
        sink.close().await?;
        debug!("sync session finished");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use futures::{Sink, Stream};
    use p2panda_core::extensions::DefaultExtensions;
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_store::{LogStore, MemoryStore, OperationStore};
    use serde::Serialize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::engine::Engine;
    use crate::protocols::{LogHeightSyncProtocol, Message};
    use crate::traits::{SyncEngine, SyncError, SyncProtocol, SyncSession};

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
    async fn protocol_impl() {
        // Create a duplex stream which simulate both ends of a bi-directional network connection
        let (peer_a, _peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);

        #[derive(Clone)]
        struct MyProtocol;
        impl SyncProtocol for MyProtocol {
            type Topic = &'static str;
            type Message = String;

            async fn run(
                self,
                _topic: Self::Topic,
                _sink: impl Sink<String, Error = SyncError>,
                _stream: impl Stream<Item = Result<String, SyncError>>,
            ) -> Result<(), SyncError> {
                Ok(())
            }
        }

        let engine = Engine {
            protocol: MyProtocol,
        };

        const TOPIC_ID: &str = "my_topic";
        let session = engine.session(peer_a_write.compat_write(), peer_a_read.compat());
        let handle = tokio::spawn(async move {
            let _ = session.run(TOPIC_ID).await.unwrap();
        });
        assert!(handle.await.is_ok());
    }

    #[tokio::test]
    async fn run_sync_strategy() {
        const TOPIC_ID: &str = "my_topic";

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
            .insert_operation(operation0.clone(), TOPIC_ID.to_string())
            .unwrap();
        store
            .insert_operation(operation1.clone(), TOPIC_ID.to_string())
            .unwrap();
        store
            .insert_operation(operation2.clone(), TOPIC_ID.to_string())
            .unwrap();

        // Create a duplex stream which simulate both ends of a bi-directional network connection
        let (peer_a, mut peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);

        // Write some message into peer_b's send buffer
        let message1: Message<DefaultExtensions> =
            Message::Have(vec![(private_key.public_key(), 0)]);
        let message2: Message<DefaultExtensions> = Message::SyncDone;
        let message_bytes = vec![message1.to_bytes(), message2.to_bytes()].concat();
        peer_b.write_all(&message_bytes[..]).await.unwrap();

        // Run the sync session (which consumes the above messages)
        let protocol = LogHeightSyncProtocol {
            sync_done_sent: false,
            sync_done_received: false,
            store: Arc::new(RwLock::new(store)),
        };
        let engine = Engine { protocol };
        let session = engine.session(peer_a_write.compat_write(), peer_a_read.compat());
        let handle = tokio::spawn(async move {
            let _ = session.run(TOPIC_ID.to_string()).await.unwrap();
        });
        handle.await.unwrap();

        // Read the entire buffer out of peer_b's read stream
        let mut buf = Vec::new();
        peer_b.read_to_end(&mut buf).await.unwrap();

        // It should contain the following two sync messages (these are the ones peer_b is
        // missing)
        let received_message0 =
            Message::<DefaultExtensions>::Have(vec![(private_key.public_key(), 2)]);
        let received_message1 =
            Message::Operation(operation1.header.clone(), operation1.body.clone());
        let received_message2 =
            Message::Operation(operation2.header.clone(), operation2.body.clone());
        let receive_message3 = Message::<DefaultExtensions>::SyncDone;
        assert_eq!(
            buf,
            [
                received_message0.to_bytes(),
                received_message1.to_bytes(),
                received_message2.to_bytes(),
                receive_message3.to_bytes()
            ]
            .concat()
        );
    }

    #[tokio::test]
    async fn sync_operation_store() {
        const TOPIC_ID: &str = "my_topic";

        // Create a store for peer a and populate it with operations
        let mut store1 = MemoryStore::default();

        let private_key1 = PrivateKey::new();
        let body = Body::new("Hello, Sloth!".as_bytes());
        let operation0 = generate_operation(&private_key1, body.clone(), 0, 0, None, None);
        let operation1 = generate_operation(
            &private_key1,
            body.clone(),
            1,
            100,
            Some(operation0.hash),
            None,
        );
        let operation2 = generate_operation(
            &private_key1,
            body.clone(),
            2,
            200,
            Some(operation1.hash),
            None,
        );

        // Insert these operations to the store using `TOPIC_ID` as the log id
        store1
            .insert_operation(operation0.clone(), TOPIC_ID.to_string())
            .unwrap();
        store1
            .insert_operation(operation1.clone(), TOPIC_ID.to_string())
            .unwrap();
        store1
            .insert_operation(operation2.clone(), TOPIC_ID.to_string())
            .unwrap();

        // Construct a log height protocol and engine for peer a
        let peer_a_protocol = LogHeightSyncProtocol {
            sync_done_sent: false,
            sync_done_received: false,
            store: Arc::new(RwLock::new(store1)),
        };
        let peer_a_engine = Engine {
            protocol: peer_a_protocol.clone(),
        };

        // Create an empty store for peer a and construct their sync protocol and engine
        let store2 = MemoryStore::default();
        let peer_b_protocol = LogHeightSyncProtocol {
            sync_done_sent: false,
            sync_done_received: false,
            store: Arc::new(RwLock::new(store2)),
        };
        let peer_b_engine = Engine {
            protocol: peer_b_protocol.clone(),
        };

        // Create a duplex stream which simulate both ends of a bi-directional network connection
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Create a sync session for both peers and spawn them in two separate threads
        let peer_a_session =
            peer_a_engine.session(peer_a_write.compat_write(), peer_a_read.compat());
        let peer_b_session =
            peer_b_engine.session(peer_b_write.compat_write(), peer_b_read.compat());

        let handle1 = tokio::spawn(async move {
            peer_a_session.run(TOPIC_ID.to_string()).await.unwrap();
        });

        let handle2 = tokio::spawn(async move {
            peer_b_session.run(TOPIC_ID.to_string()).await.unwrap();
        });

        // Wait on both to complete
        let (_, _) = tokio::join!(handle1, handle2);

        // Check log heights are now equal
        let peer_a_log_heights = peer_a_protocol
            .read_store()
            .get_log_heights(TOPIC_ID.to_string())
            .unwrap();

        let peer_b_log_heights = peer_b_protocol
            .read_store()
            .get_log_heights(TOPIC_ID.to_string())
            .unwrap();

        assert_eq!(peer_a_log_heights, peer_b_log_heights);
    }
}
