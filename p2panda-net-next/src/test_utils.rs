// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::Pin;

use futures_channel::mpsc::{self, SendError};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use p2panda_core::{Body, Hash, Header, PrivateKey, PublicKey};
use p2panda_discovery::address_book::memory::MemoryStore;
use p2panda_store::{LogStore, OperationStore};
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::TopicLogMap;
use p2panda_sync::traits::{Protocol, SyncManager};
use p2panda_sync::{FromSync, ToSync, TopicSyncManager};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::addrs::{NodeId, NodeInfo};
use crate::args::{ApplicationArguments, ArgsBuilder};
use crate::config::IrohConfig;
use crate::{NetworkId, TopicId, TransportAddress, UnsignedTransportInfo};

pub const TEST_NETWORK_ID: NetworkId = [1; 32];

pub fn test_args() -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
    NoSyncConfig,
) {
    test_args_from_seed(rand::random())
}

pub fn test_args_from_seed(
    seed: [u8; 32],
) -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
    NoSyncConfig,
) {
    let mut rng = ChaCha20Rng::from_seed(seed);
    let store = MemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());
    let private_key_bytes: [u8; 32] = rng.random();
    let (sync_config, _) = NoSyncConfig::new();
    (
        ArgsBuilder::new(TEST_NETWORK_ID)
            .with_private_key(PrivateKey::from_bytes(&private_key_bytes))
            .with_iroh_config(IrohConfig {
                bind_ip_v4: Ipv4Addr::LOCALHOST,
                bind_port_v4: rng.random_range(49152..65535),
                bind_ip_v6: Ipv6Addr::LOCALHOST,
                bind_port_v6: rng.random_range(49152..65535),
                ..Default::default()
            })
            .with_rng(rng)
            .build(),
        store,
        sync_config,
    )
}

pub fn setup_logging() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }
}

#[test]
fn deterministic_args() {
    let (args_1, _, _) = test_args_from_seed([0; 32]);
    let (args_2, _, _) = test_args_from_seed([0; 32]);
    assert_eq!(args_1.public_key, args_2.public_key);
    assert_eq!(args_1.iroh_config, args_2.iroh_config);
}

// General test types.
pub type LogId = u64;
pub type TestMemoryStore = p2panda_store::MemoryStore<LogId, ()>;
pub type TestSyncConfig = TopicSyncManagerConfig<TestMemoryStore, TestTopicMap>;
pub type TestTopicSyncManager = TopicSyncManager<TopicId, TestMemoryStore, TestTopicMap, LogId, ()>;

/// Peer abstraction used in tests.
///
/// Contains a private key, store and topic map, produces sessions for either log or topic sync
/// protocols.
pub struct App {
    pub store: TestMemoryStore,
    pub private_key: PrivateKey,
    pub topic_map: TestTopicMap,
}

impl App {
    pub fn new(id: u64) -> Self {
        let store = TestMemoryStore::new();
        let topic_map = TestTopicMap::new();
        let mut rng = <StdRng as rand::SeedableRng>::seed_from_u64(id);
        let private_key = PrivateKey::from_bytes(&rng.random());
        Self {
            store,
            private_key: private_key.clone(),
            topic_map,
        }
    }

    /// The public key of this peer.
    pub fn id(&self) -> PublicKey {
        self.private_key.public_key()
    }

    /// Create and insert an operation to the store.
    pub async fn create_operation(&mut self, body: &Body, log_id: LogId) -> (Header<()>, Vec<u8>) {
        let (header, header_bytes) = self.create_operation_no_insert(body, log_id).await;

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
    ) -> (Header<()>, Vec<u8>) {
        let (seq_num, backlink) = self
            .store
            .latest_operation(&self.private_key.public_key(), &log_id)
            .await
            .unwrap()
            .map(|(header, _)| (header.seq_num + 1, Some(header.hash())))
            .unwrap_or((0, None));

        let (header, header_bytes) =
            create_operation(&self.private_key, body, seq_num, seq_num, backlink);

        (header, header_bytes)
    }

    pub fn insert_topic(&mut self, topic: &TopicId, logs: &HashMap<PublicKey, Vec<u64>>) {
        self.topic_map.insert(topic, logs.to_owned());
    }

    pub fn sync_config(&self) -> TestSyncConfig {
        TestSyncConfig {
            store: self.store.clone(),
            topic_map: self.topic_map.clone(),
        }
    }
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
        previous: vec![],
        extensions: (),
    };
    header.sign(private_key);
    let header_bytes = header.to_bytes();
    (header, header_bytes)
}

/// Test topic map.
#[derive(Clone, Debug)]
pub struct TestTopicMap(HashMap<TopicId, HashMap<PublicKey, Vec<LogId>>>);

impl TestTopicMap {
    pub fn new() -> Self {
        TestTopicMap(HashMap::new())
    }

    pub fn insert(
        &mut self,
        topic: &TopicId,
        logs: HashMap<PublicKey, Vec<LogId>>,
    ) -> Option<HashMap<PublicKey, Vec<LogId>>> {
        self.0.insert(topic.clone(), logs)
    }
}

impl TopicLogMap<TopicId, LogId> for TestTopicMap {
    type Error = Infallible;

    async fn get(&self, topic: &TopicId) -> Result<HashMap<PublicKey, Vec<LogId>>, Self::Error> {
        Ok(self.0.get(topic).cloned().unwrap_or_default())
    }
}

pub struct NoSyncProtocol {
    session_id: u64,
    config: p2panda_sync::SyncSessionConfig<TopicId>,
    event_tx: broadcast::Sender<FromSync<NoSyncEvent>>,
}

impl Protocol for NoSyncProtocol {
    type Output = ();
    type Error = Infallible;
    type Event = NoSyncEvent;
    type Message = NoSyncMessage;

    async fn run(
        self,
        sink: &mut (impl Sink<Self::Message, Error = impl std::fmt::Debug> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, impl std::fmt::Debug>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        self.event_tx
            .send(FromSync {
                session_id: self.session_id,
                remote: self.config.remote,
                event: NoSyncEvent::SyncStarted,
            })
            .unwrap();

        sink.send(NoSyncMessage::Data).await.unwrap();

        let message = stream.next().await.unwrap().unwrap();

        self.event_tx
            .send(FromSync {
                session_id: self.session_id,
                remote: self.config.remote,
                event: NoSyncEvent::Received(message),
            })
            .unwrap();

        self.event_tx
            .send(FromSync {
                session_id: self.session_id,
                remote: self.config.remote,
                event: NoSyncEvent::SyncFinished,
            })
            .unwrap();

        // Send a close message and wait for the remote to actually close the connection.
        sink.send(NoSyncMessage::Close).await.unwrap();
        let message = stream.next().await.unwrap();
        match message {
            // We received the remote's close message and so now we close ourselves.
            Ok(NoSyncMessage::Close) => Ok(()),
            // The stream was closed by the remote.
            Err(_) => Ok(()),
            // Unexpected message.
            _ => panic!(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum NoSyncEvent {
    SessionCreated,
    SyncStarted,
    Received(NoSyncMessage),
    SyncFinished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NoSyncMessage {
    Data,
    Close,
}

#[derive(Debug)]
pub struct NoSyncManager {
    pub event_tx: broadcast::Sender<FromSync<NoSyncEvent>>,
    #[allow(unused)]
    pub event_rx: broadcast::Receiver<FromSync<NoSyncEvent>>,
}

#[derive(Clone, Debug)]
pub struct NoSyncConfig {
    pub event_tx: broadcast::Sender<FromSync<NoSyncEvent>>,
}

impl NoSyncConfig {
    pub fn new() -> (Self, broadcast::Receiver<FromSync<NoSyncEvent>>) {
        let (tx, rx) = broadcast::channel(128);
        (Self { event_tx: tx }, rx)
    }
}

impl SyncManager<TopicId> for NoSyncManager {
    type Protocol = NoSyncProtocol;
    type Config = NoSyncConfig;
    type Error = SendError;

    fn from_config(config: Self::Config) -> Self {
        let event_rx = config.event_tx.subscribe();
        NoSyncManager {
            event_tx: config.event_tx,
            event_rx,
        }
    }

    async fn session(
        &mut self,
        session_id: u64,
        config: &p2panda_sync::SyncSessionConfig<TopicId>,
    ) -> Self::Protocol {
        self.event_tx
            .send(FromSync {
                session_id,
                remote: config.remote,
                event: NoSyncEvent::SessionCreated,
            })
            .unwrap();
        NoSyncProtocol {
            session_id,
            config: config.clone(),
            event_tx: self.event_tx.clone(),
        }
    }

    async fn session_handle(
        &self,
        _session_id: u64,
    ) -> Option<std::pin::Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>> {
        // NOTE: just a dummy channel to satisfy the API in testing environment.
        let (tx, _) = mpsc::channel::<ToSync>(128);
        let sink = Box::pin(tx) as Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>;
        Some(sink)
    }

    fn subscribe(
        &self,
    ) -> impl Stream<Item = FromSync<<Self::Protocol as Protocol>::Event>> + Send + Unpin + 'static
    {
        let stream = BroadcastStream::new(self.event_tx.subscribe())
            .filter_map(|event| async { event.ok() });
        Box::pin(stream)
    }
}

pub fn generate_node_info(args: &mut ApplicationArguments) -> NodeInfo {
    let mut transport_info = UnsignedTransportInfo::from_addrs([TransportAddress::from_iroh(
        args.public_key,
        None,
        [(args.iroh_config.bind_ip_v4, args.iroh_config.bind_port_v4).into()],
    )]);
    transport_info.timestamp = args.rng.random::<u32>() as u64;
    let transport_info = transport_info.sign(&args.private_key).unwrap();
    NodeInfo {
        node_id: args.public_key,
        bootstrap: false,
        transports: Some(transport_info.into()),
    }
}
