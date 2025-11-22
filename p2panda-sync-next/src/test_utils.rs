// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{collections::HashMap, convert::Infallible};

use futures::{FutureExt, SinkExt, Stream, StreamExt};

use futures::channel::mpsc;
use p2panda_core::{Body, Extension, Hash, Header, Operation, PrivateKey, PublicKey};
use p2panda_store::{LogStore, MemoryStore, OperationStore};
use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use tokio::join;

use crate::log_sync::{LogSyncError, LogSyncEvent, LogSyncMessage, LogSyncProtocol, Logs};
use crate::topic_log_sync::TopicLogMap;
use crate::topic_log_sync::{
    TopicLogSync, TopicLogSyncError, TopicLogSyncEvent, TopicLogSyncMessage,
};
use crate::traits::Protocol;
use crate::{ToSync, TopicSyncManager};

// General test types.
pub type TestMemoryStore = MemoryStore<u64, LogIdExtension>;

// Types used in log sync protocol tests.
pub type TestLogSyncMessage = LogSyncMessage<u64>;
pub type TestLogSyncEvent = LogSyncEvent<LogIdExtension>;
pub type TestLogSync = LogSyncProtocol<u64, LogIdExtension, TestMemoryStore, TestLogSyncEvent>;
pub type TestLogSyncError = LogSyncError<u64>;

// Types used in topic log sync protocol tests.
pub type TestTopicSyncMessage = TopicLogSyncMessage<u64, LogIdExtension>;
pub type TestTopicSyncEvent = TopicLogSyncEvent<LogIdExtension>;
pub type TestTopicSync =
    TopicLogSync<TestTopic, TestMemoryStore, TestTopicMap, u64, LogIdExtension>;
pub type TestTopicSyncError = TopicLogSyncError<u64, LogIdExtension>;

pub type TestTopicSyncManager =
    TopicSyncManager<TestTopic, TestMemoryStore, TestTopicMap, u64, LogIdExtension>;

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

    /// Return a topic sync protocol.
    pub fn topic_sync_protocol(
        &mut self,
        topic: TestTopic,
        live_mode: bool,
    ) -> (
        TestTopicSync,
        mpsc::Receiver<TestTopicSyncEvent>,
        mpsc::Sender<ToSync<Operation<LogIdExtension>>>,
    ) {
        let (event_tx, event_rx) = mpsc::channel(128);
        let (live_tx, live_rx) = mpsc::channel(128);
        let live_rx = if live_mode { Some(live_rx) } else { None };
        let session = TopicLogSync::new(
            topic,
            self.store.clone(),
            self.topic_map.clone(),
            live_rx,
            event_tx,
        );
        (session, event_rx, live_tx)
    }

    /// Return a log sync protocol.
    pub fn log_sync_protocol(
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

    /// Create an operation but don't insert it in the store.
    pub async fn create_operation_no_insert(
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

        (header, header_bytes)
    }

    pub fn insert_topic(&mut self, topic: &TestTopic, logs: &HashMap<PublicKey, Vec<u64>>) {
        self.topic_map.insert(topic, logs.to_owned());
    }
}

/// Run a pair of topic sync sessions.
pub async fn run_protocol<P>(session_local: P, session_remote: P) -> Result<(), P::Error>
where
    P: Protocol + Send + Sync + 'static,
{
    let (mut local_message_tx, local_message_rx) = mpsc::channel(128);
    let (mut remote_message_tx, remote_message_rx) = mpsc::channel(128);
    let mut local_message_rx = local_message_rx.map(|message| Ok::<_, ()>(message));
    let mut remote_message_rx = remote_message_rx.map(|message| Ok::<_, ()>(message));

    let (local_result, remote_result) = join!(
        session_local.run(&mut local_message_tx, &mut remote_message_rx),
        session_remote.run(&mut remote_message_tx, &mut local_message_rx)
    );

    local_result?;
    remote_result?;
    Ok(())
}

/// Consume a vector of messages in a single topic sync session.
pub async fn run_protocol_uni<P>(
    protocol: P,
    messages: &[P::Message],
) -> Result<mpsc::Receiver<P::Message>, P::Error>
where
    P: Protocol,
    P::Message: Clone,
{
    let (mut local_message_tx, remote_message_rx) = mpsc::channel(128);
    let (mut remote_message_tx, local_message_rx) = mpsc::channel(128);
    let mut local_message_rx = local_message_rx.map(|message| Ok::<_, ()>(message));

    for message in messages {
        remote_message_tx.send(message.to_owned()).await.unwrap();
    }

    protocol
        .run(&mut local_message_tx, &mut local_message_rx)
        .await?;

    Ok(remote_message_rx)
}

pub async fn drain_stream<S>(mut stream: S) -> Vec<S::Item>
where
    S: Stream + Unpin,
{
    let mut items = Vec::new();
    while let Some(Some(item)) = stream.next().now_or_never() {
        items.push(item);
    }
    return items;
}

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
        Some(header.extensions.log_id.clone())
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
        extensions: log_id.into(),
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
