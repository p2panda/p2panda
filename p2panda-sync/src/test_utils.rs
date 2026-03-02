// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test utilities.
use std::collections::BTreeMap;

use futures::{FutureExt, SinkExt, Stream, StreamExt};

use futures::channel::mpsc;
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey, PublicKey, Topic};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteStore, Transaction, tx_unwrap};
use rand::Rng;
use rand::rngs::StdRng;
use tokio::join;
use tokio::sync::broadcast;

use crate::ToSync;
use crate::manager::TopicSyncManager;
use crate::protocols::{
    LogSync, LogSyncError, LogSyncEvent, LogSyncMessage, Logs, TopicLogSync, TopicLogSyncError,
    TopicLogSyncEvent, TopicLogSyncMessage,
};
use crate::traits::Protocol;

// Types used in log sync protocol tests.
pub type TestLogSyncMessage = LogSyncMessage<u64>;
pub type TestLogSyncEvent = LogSyncEvent<()>;
pub type TestLogSync = LogSync<u64, (), SqliteStore<'static>, TestLogSyncEvent>;
pub type TestLogSyncError = LogSyncError;

// Types used in topic log sync protocol tests.
pub type TestTopicSyncMessage = TopicLogSyncMessage<u64, ()>;
pub type TestTopicSyncEvent = TopicLogSyncEvent<()>;
pub type TestTopicSync = TopicLogSync<Topic, SqliteStore<'static>, u64, ()>;
pub type TestTopicSyncError = TopicLogSyncError;

pub type TestTopicSyncManager = TopicSyncManager<Topic, SqliteStore<'static>, u64, ()>;

/// Peer abstraction used in tests.
///
/// Contains a private key, store and topic map, produces sessions for either log or topic sync
/// protocols.
pub struct Peer {
    pub store: SqliteStore<'static>,
    pub private_key: PrivateKey,
}

impl Peer {
    pub async fn new(peer_id: u64) -> Self {
        let store = SqliteStore::temporary().await;
        let mut rng = <StdRng as rand::SeedableRng>::seed_from_u64(peer_id);
        let private_key = PrivateKey::from_bytes(&rng.random());
        Self { store, private_key }
    }

    /// The public key of this peer.
    pub fn id(&self) -> PublicKey {
        self.private_key.public_key()
    }

    /// Return a topic sync protocol.
    pub fn topic_sync_protocol(
        &mut self,
        topic: Topic,
        live_mode: bool,
    ) -> (
        TestTopicSync,
        broadcast::Receiver<TestTopicSyncEvent>,
        mpsc::Sender<ToSync<Operation<()>>>,
    ) {
        let (event_tx, event_rx) = broadcast::channel(512);
        let (live_tx, live_rx) = mpsc::channel(512);
        let live_rx = if live_mode { Some(live_rx) } else { None };
        let session = TopicLogSync::new(topic, self.store.clone(), live_rx, event_tx);
        (session, event_rx, live_tx)
    }

    /// Return a log sync protocol.
    pub fn log_sync_protocol(
        &mut self,
        logs: &Logs<u64>,
    ) -> (TestLogSync, broadcast::Receiver<TestLogSyncEvent>) {
        let (event_tx, event_rx) = broadcast::channel(512);
        let session = LogSync::new(self.store.clone(), logs.clone(), event_tx);
        (session, event_rx)
    }

    /// Create and insert an operation to the store.
    pub async fn create_operation(&mut self, body: &Body, log_id: u64) -> (Header<()>, Vec<u8>) {
        let (header, header_bytes) = self.create_operation_no_insert(body, log_id).await;

        let id = header.hash();
        let operation = Operation {
            hash: header.hash(),
            header: header.clone(),
            body: Some(body.to_owned()),
        };

        tx_unwrap!(&self.store, {
            self.store
                .insert_operation(&id, operation, log_id)
                .await
                .unwrap();
        });

        (header, header_bytes)
    }

    /// Create an operation but don't insert it in the store.
    pub async fn create_operation_no_insert(
        &mut self,
        body: &Body,
        log_id: u64,
    ) -> (Header<()>, Vec<u8>) {
        let (seq_num, backlink) = <SqliteStore as LogStore<
            Operation<()>,
            PublicKey,
            u64,
            u64,
            p2panda_core::Hash,
        >>::get_latest_entry(
            &self.store, &self.private_key.public_key(), &log_id
        )
        .await
        .unwrap()
        .map(|(hash, seq_num)| (seq_num + 1, Some(hash)))
        .unwrap_or((0, None));

        let (header, header_bytes) =
            create_operation(&self.private_key, body, seq_num, rand::random(), backlink);

        (header, header_bytes)
    }

    pub async fn associate(&mut self, topic: &Topic, logs: &BTreeMap<PublicKey, Vec<u64>>) {
        let permit = self.store.begin().await.unwrap();
        for (author, logs) in logs {
            for log_id in logs {
                self.store.associate(topic, author, log_id).await.unwrap();
            }
        }
        self.store.commit(permit).await.unwrap();
    }
}

pub fn setup_logging() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }
}

/// Run a pair of topic sync sessions.
pub async fn run_protocol<P>(session_local: P, session_remote: P) -> Result<(), P::Error>
where
    P: Protocol + Send + 'static,
{
    let (mut local_message_tx, local_message_rx) = mpsc::channel(512);
    let (mut remote_message_tx, remote_message_rx) = mpsc::channel(512);
    let mut local_message_rx = local_message_rx.map(Ok::<_, ()>);
    let mut remote_message_rx = remote_message_rx.map(Ok::<_, ()>);

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
    let (mut local_message_tx, remote_message_rx) = mpsc::channel(512);
    let (mut remote_message_tx, local_message_rx) = mpsc::channel(512);
    let mut local_message_rx = local_message_rx.map(Ok::<_, ()>);

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
    items
}

/// Create a single operation.
pub fn create_operation(
    private_key: &PrivateKey,
    body: &Body,
    seq_num: u64,
    timestamp: u64,
    backlink: Option<Hash>,
) -> (Header<()>, Vec<u8>) {
    let mut header = Header::<()> {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp,
        seq_num,
        backlink,
        extensions: (),
    };
    header.sign(private_key);
    let header_bytes = header.to_bytes();
    (header, header_bytes)
}
