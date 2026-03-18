// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

use p2panda_core::PrivateKey;
use p2panda_net::discovery::DiscoveryConfig;
use p2panda_net::gossip::GossipConfig;
use p2panda_net::iroh_endpoint::RelayUrl;
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::{NetworkId, NodeId};
use p2panda_store::SqliteStore;
use p2panda_store::sqlite::{SqlitePool, SqliteStoreBuilder};

use crate::Node;
use crate::forge::OperationForge;
use crate::node::{AckPolicy, Config, SpawnError};
use crate::processor::{Pipeline, TaskTracker};

#[derive(Default)]
pub struct NodeBuilder {
    private_key: Option<PrivateKey>,
    config: Config,
    store_options: StoreBuilderOptions,
}

impl NodeBuilder {
    pub fn new() -> Self {
        NodeBuilder {
            private_key: None,
            config: Config::default(),
            store_options: StoreBuilderOptions::default(),
        }
    }

    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn database_url(mut self, url: &str) -> Self {
        self.store_options = StoreBuilderOptions::Url(url.to_string());
        self
    }

    pub fn database_pool(mut self, pool: SqlitePool) -> Self {
        self.store_options = StoreBuilderOptions::Pool(pool);
        self
    }

    pub fn ack_policy(mut self, value: AckPolicy) -> Self {
        self.config.ack_policy = value;
        self
    }

    pub fn network_id(mut self, network_id: NetworkId) -> Self {
        self.config.network.network_id = network_id;
        self
    }

    pub fn relay_url(mut self, url: RelayUrl) -> Self {
        self.config.network.relay_urls.insert(url);
        self
    }

    pub fn bootstrap(mut self, node_id: NodeId) -> Self {
        self.config.network.bootstraps.insert(node_id);
        self
    }

    pub fn mdns_mode(mut self, mode: MdnsDiscoveryMode) -> Self {
        self.config.network.mdns_mode = mode;
        self
    }

    pub fn bind_ip_v4(mut self, ip: Ipv4Addr) -> Self {
        self.config.network.iroh.bind_ip_v4 = ip;
        self
    }

    pub fn bind_port_v4(mut self, port: u16) -> Self {
        self.config.network.iroh.bind_port_v4 = port;
        self
    }

    pub fn bind_ip_v6(mut self, ip: Ipv6Addr) -> Self {
        self.config.network.iroh.bind_ip_v6 = ip;
        self
    }

    pub fn bind_port_v6(mut self, port: u16) -> Self {
        self.config.network.iroh.bind_port_v6 = port;
        self
    }

    pub fn discovery_config(mut self, config: DiscoveryConfig) -> Self {
        self.config.network.discovery = config;
        self
    }

    pub fn gossip_config(mut self, config: GossipConfig) -> Self {
        self.config.network.gossip = config;
        self
    }

    pub async fn spawn(self) -> Result<Node, SpawnError> {
        let private_key = self.private_key.unwrap_or_default();
        let store = match self.store_options {
            StoreBuilderOptions::Memory => SqliteStoreBuilder::new().build().await?,
            StoreBuilderOptions::Url(url) => {
                SqliteStoreBuilder::new().database_url(&url).build().await?
            }
            StoreBuilderOptions::Pool(pool) => SqliteStore::from_pool(pool),
        };
        let forge = OperationForge::from_private_key(private_key, store.clone());

        let tasks = TaskTracker::new();
        let pipeline = Pipeline::new::<SqliteStore>(store.clone(), tasks);

        let node = Node::spawn_inner(self.config, store, forge, pipeline).await?;

        Ok(node)
    }
}

#[derive(Default)]
enum StoreBuilderOptions {
    #[default]
    Memory,
    Url(String),
    Pool(SqlitePool),
}
