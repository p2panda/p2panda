// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use p2panda_core::{Body, Hash, Header, PrivateKey, PublicKey};
use p2panda_discovery::address_book::memory::MemoryStore;
use p2panda_store::{LogStore, OperationStore};
use p2panda_sync::manager::{TopicSyncManager, TopicSyncManagerArgs};
use p2panda_sync::protocols::Logs;
use p2panda_sync::traits::TopicMap;
use ractor::thread_local::ThreadLocalActorSpawner;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use tokio::sync::RwLock;

use crate::addrs::{NodeInfo, NodeMetrics, TransportAddress, TrustedTransportInfo};
use crate::discovery::DiscoveryConfig;
use crate::iroh_endpoint::IrohConfig;
use crate::iroh_mdns::MdnsDiscoveryMode;
use crate::{
    AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NetworkId, NodeId, TopicId,
};

pub const TEST_NETWORK_ID: NetworkId = [1; 32];

#[derive(Clone, Debug)]
pub struct ApplicationArguments {
    pub network_id: NetworkId,
    pub rng: ChaCha20Rng,
    pub private_key: PrivateKey,
    pub public_key: PublicKey,
    pub iroh_config: IrohConfig,
    pub discovery_config: DiscoveryConfig,
    pub mdns_mode: MdnsDiscoveryMode,
    pub root_thread_pool: ThreadLocalActorSpawner,
}

impl ApplicationArguments {
    pub fn node_info(&mut self) -> NodeInfo {
        let transport_info = TrustedTransportInfo::from_addrs([TransportAddress::from_iroh(
            self.public_key,
            None,
            [(self.iroh_config.bind_ip_v4, self.iroh_config.bind_port_v4).into()],
        )]);

        NodeInfo {
            node_id: self.public_key,
            bootstrap: false,
            transports: Some(transport_info.into()),
            metrics: NodeMetrics::default(),
        }
    }
}

pub struct ArgsBuilder {
    network_id: NetworkId,
    rng: Option<ChaCha20Rng>,
    private_key: Option<PrivateKey>,
    iroh_config: Option<IrohConfig>,
    discovery_config: Option<DiscoveryConfig>,
    mdns_mode: Option<MdnsDiscoveryMode>,
}

impl ArgsBuilder {
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            rng: None,
            private_key: None,
            iroh_config: None,
            discovery_config: None,
            mdns_mode: None,
        }
    }

    pub fn with_network_id(mut self, network_id: NetworkId) -> Self {
        self.network_id = network_id;
        self
    }

    pub fn with_rng(mut self, rng: ChaCha20Rng) -> Self {
        self.rng = Some(rng);
        self
    }

    pub fn with_iroh_config(mut self, config: IrohConfig) -> Self {
        self.iroh_config = Some(config);
        self
    }

    pub fn with_mdns_mode(mut self, mode: MdnsDiscoveryMode) -> Self {
        self.mdns_mode = Some(mode);
        self
    }

    pub fn with_discovery_config(mut self, config: DiscoveryConfig) -> Self {
        self.discovery_config = Some(config);
        self
    }

    pub fn with_private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn build(self) -> ApplicationArguments {
        let private_key = self.private_key.unwrap_or_default();
        ApplicationArguments {
            network_id: self.network_id,
            rng: self.rng.unwrap_or(ChaCha20Rng::from_os_rng()),
            public_key: private_key.public_key(),
            private_key,
            iroh_config: self.iroh_config.unwrap_or_default(),
            discovery_config: self.discovery_config.unwrap_or_default(),
            mdns_mode: self.mdns_mode.unwrap_or_default(),
            root_thread_pool: ThreadLocalActorSpawner::new(),
        }
    }
}

pub fn test_args() -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
) {
    test_args_from_seed(rand::random())
}

pub fn test_args_from_seed(
    seed: [u8; 32],
) -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
) {
    let mut rng = ChaCha20Rng::from_seed(seed);
    let store = MemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());
    let private_key_bytes: [u8; 32] = rng.random();
    (
        ArgsBuilder::new(TEST_NETWORK_ID)
            .with_private_key(PrivateKey::from_bytes(&private_key_bytes))
            .with_iroh_config(IrohConfig {
                bind_ip_v4: Ipv4Addr::LOCALHOST,
                bind_port_v4: rng.random_range(49152..65535),
                bind_ip_v6: Ipv6Addr::LOCALHOST,
                bind_port_v6: rng.random_range(49152..65535),
            })
            .with_rng(rng)
            .with_mdns_mode(MdnsDiscoveryMode::Passive)
            .build(),
        store,
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
    let (args_1, _) = test_args_from_seed([0; 32]);
    let (args_2, _) = test_args_from_seed([0; 32]);
    assert_eq!(args_1.public_key, args_2.public_key);
    assert_eq!(args_1.iroh_config, args_2.iroh_config);
}

pub struct TestNode {
    pub args: ApplicationArguments,
    pub client: TestClient,
    pub address_book: AddressBook,
    pub mdns: MdnsDiscovery,
    pub endpoint: Endpoint,
    pub discovery: Discovery,
    pub gossip: Gossip,
    pub log_sync: LogSync<TestMemoryStore, TestLogId, TestExtensions, TestTopicMap>,
}

impl TestNode {
    pub async fn spawn(seed: [u8; 32]) -> Self {
        Self::spawn_with_args(test_args_from_seed(seed)).await
    }

    pub async fn spawn_with_args(
        mut args: (
            ApplicationArguments,
            MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
        ),
    ) -> Self {
        let client = TestClient::new(
            // The identity of the "author" or client has a different private key from the node.
            PrivateKey::from_bytes(&args.0.rng.random::<[u8; 32]>()),
        );

        Self::spawn_with_args_and_client(args, client).await
    }

    pub async fn spawn_with_args_and_client(
        args: (
            ApplicationArguments,
            MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
        ),
        client: TestClient,
    ) -> Self {
        let (args, address_book_store) = args;

        let address_book = AddressBook::builder()
            .store(address_book_store)
            .spawn()
            .await
            .unwrap();

        let endpoint = Endpoint::builder(address_book.clone())
            .config(args.iroh_config.clone())
            .private_key(args.private_key.clone())
            .spawn()
            .await
            .unwrap();

        let mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
            .mode(args.mdns_mode.clone())
            .spawn()
            .await
            .unwrap();

        let discovery = Discovery::builder(address_book.clone(), endpoint.clone())
            .config(args.discovery_config.clone())
            .spawn()
            .await
            .unwrap();

        let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
            .spawn()
            .await
            .unwrap();

        let (operation_store, topic_map) = client.sync_args();

        let log_sync =
            LogSync::builder(operation_store, topic_map, endpoint.clone(), gossip.clone())
                .spawn()
                .await
                .unwrap();

        Self {
            args,
            client,
            address_book,
            mdns,
            discovery,
            endpoint,
            gossip,
            log_sync,
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    pub fn node_info(&mut self) -> NodeInfo {
        self.args.node_info()
    }

    pub fn client_id(&self) -> PublicKey {
        self.client.id()
    }
}

pub type TestExtensions = ();

pub type TestLogId = u64;

pub type TestMemoryStore = p2panda_store::MemoryStore<TestLogId, TestExtensions>;

pub type TestSyncConfig = TopicSyncManagerArgs<TestMemoryStore, TestTopicMap>;

pub type TestTopicSyncManager =
    TopicSyncManager<TopicId, TestMemoryStore, TestTopicMap, TestLogId, TestExtensions>;

/// Client abstraction used in tests.
///
/// Contains a private key, store and topic map, produces sessions for either log or topic sync
/// protocols.
#[derive(Clone)]
pub struct TestClient {
    pub store: TestMemoryStore,
    pub private_key: PrivateKey,
    pub topic_map: TestTopicMap,
}

impl TestClient {
    pub fn new(private_key: PrivateKey) -> Self {
        let store = TestMemoryStore::new();
        let topic_map = TestTopicMap::new();

        Self {
            store,
            private_key,
            topic_map,
        }
    }

    /// The public key of this client.
    pub fn id(&self) -> PublicKey {
        self.private_key.public_key()
    }

    /// Create and insert an operation to the store.
    pub async fn create_operation(
        &mut self,
        body: &[u8],
        log_id: TestLogId,
    ) -> (Header<()>, Vec<u8>, Body) {
        let (header, header_bytes, body) = self.create_operation_no_insert(body, log_id).await;

        self.store
            .insert_operation(header.hash(), &header, Some(&body), &header_bytes, &log_id)
            .await
            .unwrap();

        (header, header_bytes, body)
    }

    /// Create an operation but don't insert it in the store.
    pub async fn create_operation_no_insert(
        &mut self,
        body: &[u8],
        log_id: u64,
    ) -> (Header<()>, Vec<u8>, Body) {
        let (seq_num, backlink) = self
            .store
            .latest_operation(&self.private_key.public_key(), &log_id)
            .await
            .unwrap()
            .map(|(header, _)| (header.seq_num + 1, Some(header.hash())))
            .unwrap_or((0, None));

        let (header, header_bytes, body) =
            create_operation(&self.private_key, body, seq_num, seq_num, backlink);

        (header, header_bytes, body)
    }

    pub async fn insert_topic(&mut self, topic: &TopicId, logs: HashMap<PublicKey, Vec<u64>>) {
        self.topic_map.insert(topic, logs).await;
    }

    pub fn sync_args(&self) -> (TestMemoryStore, TestTopicMap) {
        (self.store.clone(), self.topic_map.clone())
    }
}

/// Create a single operation.
pub fn create_operation(
    private_key: &PrivateKey,
    body: &[u8],
    seq_num: u64,
    timestamp: u64,
    backlink: Option<Hash>,
) -> (Header<TestExtensions>, Vec<u8>, Body) {
    let body = Body::new(body);

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

    (header, header_bytes, body)
}

/// Test topic map.
#[derive(Clone, Debug)]
#[allow(clippy::type_complexity)]
pub struct TestTopicMap(Arc<RwLock<HashMap<TopicId, Logs<TestLogId>>>>);

impl TestTopicMap {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    pub async fn insert(
        &mut self,
        topic: &TopicId,
        logs: Logs<TestLogId>,
    ) -> Option<Logs<TestLogId>> {
        let mut map = self.0.write().await;
        map.insert(*topic, logs)
    }
}

impl TopicMap<TopicId, Logs<TestLogId>> for TestTopicMap {
    type Error = Infallible;

    async fn get(&self, topic: &TopicId) -> Result<Logs<TestLogId>, Self::Error> {
        let map = self.0.read().await;
        Ok(map.get(topic).cloned().unwrap_or_default())
    }
}
