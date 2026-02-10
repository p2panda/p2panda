// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_net::iroh_endpoint::RelayUrl;
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::{NetworkId, NodeId};
use p2panda_store::sqlite::SqliteStoreBuilder;

use crate::Node;
use crate::network::NetworkConfig;
use crate::node::{Config, NodeError};

#[derive(Default)]
pub struct NodeBuilder {
    private_key: Option<PrivateKey>,
    config: Config,
    store: SqliteStoreBuilder,
}

impl NodeBuilder {
    pub(crate) fn new() -> Self {
        NodeBuilder {
            private_key: None,
            config: Config::default(),
            store: SqliteStoreBuilder::default(),
        }
    }

    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn database_url(mut self, url: &str) -> Self {
        self.store = self.store.database_url(url);
        self
    }

    // TODO: Check if this is sufficient for Reflection to run custom migrations. Are we exporting
    // the p2panda one's already in p2panda-store-next?
    pub fn default_migrations(mut self, value: bool) -> Self {
        self.store = self.store.run_default_migrations(value);
        self
    }

    pub fn auto_commit(mut self, value: bool) -> Self {
        self.config.auto_commit = value;
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

    // TODO: Add more ways to configure the network, etc.

    pub async fn spawn(self) -> Result<Node, NodeError> {
        let private_key = self.private_key.unwrap_or_default();
        let store = self.store.build().await?;

        Node::spawn_inner(self.config, private_key, store).await
    }
}
