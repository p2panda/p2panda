// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use p2panda_core::{Body, Hash, Header, Operation, SeqNum, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteStore, Transaction, tx_unwrap};
use p2panda_sync::manager::TopicSyncManager;
use ractor::thread_local::ThreadLocalActorSpawner;
use rand::rngs::SysRng;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::addrs::{NodeInfo, NodeMetrics, TransportAddress, TrustedTransportInfo};
use crate::discovery::DiscoveryConfig;
use crate::iroh_endpoint::IrohConfig;
use crate::iroh_mdns::MdnsDiscoveryMode;
use crate::{AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NetworkId, NodeId};

pub const TEST_NETWORK_ID: NetworkId = [1; 32];

#[derive(Clone, Debug)]
pub struct ApplicationArguments {
    pub network_id: NetworkId,
    pub rng: ChaCha20Rng,
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    pub iroh_config: IrohConfig,
    pub discovery_config: DiscoveryConfig,
    pub mdns_mode: MdnsDiscoveryMode,
    pub root_thread_pool: ThreadLocalActorSpawner,
}

impl ApplicationArguments {
    pub fn node_info(&mut self) -> NodeInfo {
        let transport_info = TrustedTransportInfo::from_addrs([TransportAddress::from_iroh(
            self.verifying_key,
            None,
            [(self.iroh_config.bind_ip_v4, self.iroh_config.bind_port_v4).into()],
        )]);

        NodeInfo {
            node_id: self.verifying_key,
            bootstrap: false,
            transports: Some(transport_info.into()),
            metrics: NodeMetrics::default(),
        }
    }
}

pub struct ArgsBuilder {
    network_id: NetworkId,
    rng: Option<ChaCha20Rng>,
    signing_key: Option<SigningKey>,
    iroh_config: Option<IrohConfig>,
    discovery_config: Option<DiscoveryConfig>,
    mdns_mode: Option<MdnsDiscoveryMode>,
}

impl ArgsBuilder {
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            rng: None,
            signing_key: None,
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

    pub fn with_signing_key(mut self, signing_key: SigningKey) -> Self {
        self.signing_key = Some(signing_key);
        self
    }

    pub fn build(self) -> ApplicationArguments {
        let signing_key = self.signing_key.unwrap_or_default();
        ApplicationArguments {
            network_id: self.network_id,
            rng: self
                .rng
                .unwrap_or(ChaCha20Rng::try_from_rng(&mut SysRng).unwrap()),
            verifying_key: signing_key.verifying_key(),
            signing_key,
            iroh_config: self.iroh_config.unwrap_or_default(),
            discovery_config: self.discovery_config.unwrap_or_default(),
            mdns_mode: self.mdns_mode.unwrap_or_default(),
            root_thread_pool: ThreadLocalActorSpawner::new(),
        }
    }
}

pub fn test_args() -> ApplicationArguments {
    test_args_from_seed(rand::random())
}

pub fn test_args_from_seed(seed: [u8; 32]) -> ApplicationArguments {
    let mut rng = ChaCha20Rng::from_seed(seed);
    let signing_key_bytes: [u8; 32] = rng.random();
    ArgsBuilder::new(TEST_NETWORK_ID)
        .with_signing_key(SigningKey::from_bytes(&signing_key_bytes))
        .with_iroh_config(IrohConfig {
            bind_ip_v4: Ipv4Addr::LOCALHOST,
            bind_port_v4: rng.random_range(49152..65535),
            bind_ip_v6: Ipv6Addr::LOCALHOST,
            bind_port_v6: rng.random_range(49152..65535),
        })
        .with_rng(rng)
        .with_mdns_mode(MdnsDiscoveryMode::Passive)
        .build()
}

#[test]
fn deterministic_args() {
    let args_1 = test_args_from_seed([0; 32]);
    let args_2 = test_args_from_seed([0; 32]);
    assert_eq!(args_1.verifying_key, args_2.verifying_key);
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
    pub log_sync: LogSync<SqliteStore, TestLogId, TestExtensions>,
}

impl TestNode {
    pub async fn spawn(seed: [u8; 32], node_info: Option<NodeInfo>) -> Self {
        Self::spawn_with_args(test_args_from_seed(seed), node_info).await
    }

    pub async fn spawn_with_args(
        mut args: ApplicationArguments,
        node_info: Option<NodeInfo>,
    ) -> Self {
        let client = TestClient::new(
            // The identity of the "author" or client has a different private key from the node.
            SigningKey::from_bytes(&args.rng.random::<[u8; 32]>()),
        )
        .await;

        let address_book = AddressBook::builder().spawn().await.unwrap();

        // Insert provided node info into the address book.
        //
        // This is useful for informing the local node of a remote node manually, before the
        // discovery services have been spawned.
        if let Some(info) = node_info {
            address_book.insert_node_info(info).await.unwrap();
        }

        let endpoint = Endpoint::builder(address_book.clone())
            .config(args.iroh_config.clone())
            .signing_key(args.signing_key.clone())
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

        let log_sync: LogSync<SqliteStore, u64, ()> =
            LogSync::builder(client.store.clone(), endpoint.clone(), gossip.clone())
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
        self.args.verifying_key
    }

    pub fn node_info(&mut self) -> NodeInfo {
        self.args.node_info()
    }

    pub fn client_id(&self) -> VerifyingKey {
        self.client.id()
    }
}

pub type TestExtensions = ();

pub type TestLogId = u64;

pub type TestTopicSyncManager = TopicSyncManager<Topic, SqliteStore, TestLogId, TestExtensions>;

/// Client abstraction used in tests.
///
/// Contains a private key, store and topic map, produces sessions for either log or topic sync
/// protocols.
#[derive(Clone)]
pub struct TestClient {
    pub store: SqliteStore,
    pub signing_key: SigningKey,
}

impl TestClient {
    pub async fn new(signing_key: SigningKey) -> Self {
        let store = SqliteStore::temporary().await;

        Self { store, signing_key }
    }

    /// The public key of this client.
    pub fn id(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Create and insert an operation to the store.
    pub async fn create_operation(
        &mut self,
        body: &[u8],
        log_id: TestLogId,
    ) -> (Header<()>, Vec<u8>, Body) {
        let (header, header_bytes, body) = self.create_operation_no_insert(body, log_id).await;

        let id = header.hash();
        let operation = Operation {
            hash: header.hash(),
            header: header.clone(),
            body: Some(body.to_owned()),
        };

        tx_unwrap!(&self.store, {
            self.store
                .insert_operation(&id, &operation, &log_id)
                .await
                .unwrap();
        });

        (header, header_bytes, body)
    }

    /// Create an operation but don't insert it in the store.
    pub async fn create_operation_no_insert(
        &mut self,
        body: &[u8],
        log_id: u64,
    ) -> (Header<()>, Vec<u8>, Body) {
        let (header, header_bytes, body) = tx_unwrap!(&self.store, {
            let (seq_num, backlink) = <SqliteStore as LogStore<
                Operation<TestExtensions>,
                VerifyingKey,
                u64,
                SeqNum,
                p2panda_core::Hash,
            >>::get_latest_entry_tx(
                &self.store, &self.signing_key.verifying_key(), &log_id
            )
            .await
            .unwrap()
            .map(|operation| (operation.header.seq_num + 1, Some(operation.hash)))
            .unwrap_or((0, None));

            create_operation(&self.signing_key, body, seq_num, backlink)
        });

        (header, header_bytes, body)
    }

    pub async fn associate(&mut self, topic: &Topic, logs: &HashMap<VerifyingKey, Vec<u64>>) {
        let permit = self.store.begin().await.unwrap();
        for (author, logs) in logs {
            for log_id in logs {
                self.store.associate(topic, author, log_id).await.unwrap();
            }
        }
        self.store.commit(permit).await.unwrap();
    }
}

/// Create a single operation.
pub fn create_operation(
    signing_key: &SigningKey,
    body: &[u8],
    seq_num: SeqNum,
    backlink: Option<Hash>,
) -> (Header<TestExtensions>, Vec<u8>, Body) {
    let body = Body::new(body);

    let mut header = Header::<()> {
        version: 1,
        verifying_key: signing_key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        seq_num,
        backlink,
        extensions: (),
    };

    header.sign(signing_key);
    let header_bytes = header.to_bytes();

    (header, header_bytes, body)
}
