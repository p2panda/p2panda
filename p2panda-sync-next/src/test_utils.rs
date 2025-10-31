// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;
use std::{collections::HashMap, convert::Infallible};

use futures::{SinkExt, StreamExt};

use futures::channel::mpsc;
use p2panda_core::PublicKey;
use p2panda_core::cbor::encode_cbor;
use p2panda_core::{Body, Extension, Hash, Header, PrivateKey};
use p2panda_store::{LogStore, MemoryStore, OperationStore};
use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncWriteExt, DuplexStream, ReadHalf};
use tokio::sync::broadcast;
use tokio::time::sleep;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::cbor::into_cbor_stream;
use crate::log_sync::{LogSyncError, LogSyncEvent, LogSyncMessage, LogSyncProtocol, Logs};
use crate::topic_log_sync::TopicLogMap;
use crate::topic_log_sync::{
    LiveModeMessage, Role, TopicLogSync, TopicLogSyncError, TopicLogSyncEvent, TopicLogSyncMessage,
};
use crate::traits::TopicQuery;
use crate::traits::{Protocol, SyncProtocol};

// General test types.
pub type TestMemoryStore = MemoryStore<u64, LogIdExtension>;

// Types used in log sync protocol tests.
pub type TestLogSyncMessage = LogSyncMessage<u64>;
pub type TestLogSyncEvent = LogSyncEvent<LogIdExtension>;
pub type TestLogSync = LogSyncProtocol<u64, LogIdExtension, TestMemoryStore, TestLogSyncEvent>;
pub type TestLogSyncError = LogSyncError<u64, LogIdExtension, TestMemoryStore>;

// Types used in topic log sync protocol tests.
pub type TestTopicSyncMessage = TopicLogSyncMessage<TestTopic, u64, LogIdExtension>;
pub type TestTopicSyncEvent = TopicLogSyncEvent<TestTopic, LogIdExtension>;
pub type TestTopicSync =
    TopicLogSync<TestTopic, TestMemoryStore, TestTopicMap, u64, LogIdExtension>;
pub type TestTopicSyncError =
    TopicLogSyncError<TestTopic, TestMemoryStore, TestTopicMap, u64, LogIdExtension>;

/// Peer abstraction used in tests.
///
/// Contains a private key, store and topic map, produces sessions for either log or topic sync
/// protocols.
pub struct Peer {
    pub store: TestMemoryStore,
    pub private_key: PrivateKey,
    pub topic_map: TestTopicMap,
}

impl Peer {
    pub fn new(peer_id: u64) -> Self {
        let store = TestMemoryStore::new();
        let topic_map = TestTopicMap::new();
        let mut rng = <StdRng as rand::SeedableRng>::seed_from_u64(peer_id);
        let private_key = PrivateKey::from_bytes(&rng.random());
        Self {
            store,
            private_key,
            topic_map,
        }
    }

    /// The public key of this peer.
    pub fn id(&self) -> PublicKey {
        self.private_key.public_key()
    }

    /// Return a topic sync session.
    pub fn topic_sync_session(
        &mut self,
        role: Role<TestTopic>,
        live_mode: bool,
    ) -> (
        TestTopicSync,
        mpsc::Receiver<TestTopicSyncEvent>,
        broadcast::Sender<LiveModeMessage<LogIdExtension>>,
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

    /// Return a log sync session.
    pub fn log_sync_session(
        &mut self,
        logs: &Logs<u64>,
    ) -> (TestLogSync, mpsc::Receiver<TestLogSyncEvent>) {
        let (event_tx, event_rx) = mpsc::channel(128);
        let session = LogSyncProtocol::new(self.store.clone(), logs.clone(), event_tx);
        (session, event_rx)
    }

    /// Create and insert an operation to the store.
    pub async fn create_operation(
        &mut self,
        body: &Body,
        log_id: u64,
    ) -> (Header<LogIdExtension>, Vec<u8>) {
        let (seq_num, backlink) = self
            .store
            .latest_operation(&self.private_key.public_key(), &log_id)
            .await
            .unwrap()
            .map(|(header, _)| (header.seq_num + 1, Some(header.hash())))
            .unwrap_or((0, None));

        let (header, header_bytes) =
            create_operation(&self.private_key, body, seq_num, seq_num, backlink, log_id);

        self.store
            .insert_operation(header.hash(), &header, Some(body), &header_bytes, &log_id)
            .await
            .unwrap();
        (header, header_bytes)
    }

    pub fn insert_topic(&mut self, topic: &TestTopic, logs: &HashMap<PublicKey, Vec<u64>>) {
        self.topic_map.insert(topic, logs.to_owned());
    }
}

/// Run a pair of log sync sessions.
pub async fn run_log_sync(
    session_local: TestLogSync,
    session_remote: TestLogSync,
) -> Result<(), TestLogSyncError> {
    let (mut local_message_tx, local_message_rx) = mpsc::channel(128);
    let (mut remote_message_tx, remote_message_rx) = mpsc::channel(128);
    let mut local_message_rx = local_message_rx.map(|message| Ok::<_, ()>(message));
    let mut remote_message_rx = remote_message_rx.map(|message| Ok::<_, ()>(message));

    let local_task = tokio::spawn(async move {
        session_local
            .run(&mut local_message_tx, &mut remote_message_rx)
            .await
    });

    let remote_task = tokio::spawn(async move {
        session_remote
            .run(&mut remote_message_tx, &mut local_message_rx)
            .await
    });
    let (local_result, remote_result) = tokio::try_join!(local_task, remote_task).unwrap();
    local_result?;
    remote_result?;
    Ok(())
}

/// Consume a vector of messages in a single log sync session.
pub async fn run_log_sync_uni(
    session: TestLogSync,
    messages: &[TestLogSyncMessage],
) -> Result<mpsc::Receiver<TestLogSyncMessage>, TestLogSyncError> {
    let (mut local_message_tx, remote_message_rx) = mpsc::channel(128);
    let (mut remote_message_tx, local_message_rx) = mpsc::channel(128);
    let mut local_message_rx = local_message_rx.map(|message| Ok::<_, ()>(message));

    for message in messages {
        remote_message_tx.send(message.to_owned()).await.unwrap();
    }

    session
        .run(&mut local_message_tx, &mut local_message_rx)
        .await?;

    Ok(remote_message_rx)
}

/// Run a pair of topic sync sessions.
pub async fn run_topic_sync(
    session_local: TestTopicSync,
    session_remote: TestTopicSync,
) -> Result<(), TestTopicSyncError> {
    let (local_bi_streams, remote_bi_streams) = tokio::io::duplex(64 * 1024);
    let (local_read, local_write) = tokio::io::split(local_bi_streams);
    let (remote_read, remote_write) = tokio::io::split(remote_bi_streams);

    let local_task = tokio::spawn(async move {
        session_local
            .run(&mut local_write.compat_write(), &mut local_read.compat())
            .await
    });

    let remote_task = tokio::spawn(async move {
        session_remote
            .run(&mut remote_write.compat_write(), &mut remote_read.compat())
            .await
    });
    let (local_result, remote_result) = tokio::try_join!(local_task, remote_task).unwrap();
    local_result?;
    remote_result?;
    Ok(())
}

/// Consume a vector of messages in a single topic sync session.
pub async fn run_topic_sync_uni(
    session_local: TestTopicSync,
    messages: &[TestTopicSyncMessage],
) -> Result<ReadHalf<DuplexStream>, TestTopicSyncError> {
    let (local_bi_streams, remote_bi_streams) = tokio::io::duplex(64 * 1024);
    let (local_read, local_write) = tokio::io::split(local_bi_streams);
    let (remote_read, mut remote_write) = tokio::io::split(remote_bi_streams);

    for message in messages {
        remote_write
            .write(&encode_cbor(message).unwrap()[..])
            .await
            .unwrap();
    }

    session_local
        .run(&mut local_write.compat_write(), &mut local_read.compat())
        .await?;

    Ok(remote_read)
}

/// Receive all messages in a topic sync session read stream.
pub async fn topic_sync_recv_all(read: ReadHalf<DuplexStream>) -> Vec<TestTopicSyncMessage> {
    let idle_timeout = Duration::from_millis(200);
    let mut messages = vec![];
    let mut read_compat = read.compat();
    let mut stream = into_cbor_stream(&mut read_compat);
    loop {
        let timeout = sleep(idle_timeout);
        tokio::select! {
            biased;
            Some(Ok(message)) = stream.next() => messages.push(message),
            _ = timeout =>  break
        }
    }
    messages
}
//
// impl From<mpsc::SendError> for TestLogSyncError {
//     fn from(err: mpsc::SendError) -> Self {
//         TestLogSyncError::MessageSink(format!("{err:?}"))
//     }
// }

/// Log id extension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogId(u64);

impl From<u64> for LogId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<LogId> for u64 {
    fn from(value: LogId) -> Self {
        value.0
    }
}

impl From<u64> for LogIdExtension {
    fn from(value: u64) -> Self {
        Self {
            log_id: value.into(),
        }
    }
}

/// Extensions containing only a log id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogIdExtension {
    pub log_id: LogId,
}

impl Extension<LogId> for LogIdExtension {
    fn extract(header: &Header<Self>) -> Option<LogId> {
        let Some(extensions) = header.extensions.as_ref() else {
            return None;
        };

        Some(extensions.log_id.clone())
    }
}

/// Create a single operation.
pub fn create_operation(
    private_key: &PrivateKey,
    body: &Body,
    seq_num: u64,
    timestamp: u64,
    backlink: Option<Hash>,
    log_id: u64,
) -> (Header<LogIdExtension>, Vec<u8>) {
    let mut header = Header::<LogIdExtension> {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp,
        seq_num,
        backlink,
        previous: vec![],
        extensions: Some(log_id.into()),
    };
    header.sign(private_key);
    let header_bytes = header.to_bytes();
    (header, header_bytes)
}

/// Test topic.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct TestTopic(String);

impl TestTopic {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }
}

impl TopicQuery for TestTopic {}

/// Test topic map.
#[derive(Clone, Debug)]
pub struct TestTopicMap(HashMap<TestTopic, Logs<u64>>);

impl TestTopicMap {
    pub fn new() -> Self {
        TestTopicMap(HashMap::new())
    }

    pub fn insert(&mut self, topic_query: &TestTopic, logs: Logs<u64>) -> Option<Logs<u64>> {
        self.0.insert(topic_query.clone(), logs)
    }
}

impl TopicLogMap<TestTopic, u64> for TestTopicMap {
    type Error = Infallible;

    async fn get(&self, topic_query: &TestTopic) -> Result<Logs<u64>, Self::Error> {
        Ok(self.0.get(topic_query).cloned().unwrap_or_default())
    }
}
