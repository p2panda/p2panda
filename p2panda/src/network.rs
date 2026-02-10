// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

use p2panda_core::PrivateKey;
use p2panda_net::address_book::AddressBookError;
use p2panda_net::addrs::NodeInfo;
use p2panda_net::discovery::{DiscoveryConfig, DiscoveryError};
use p2panda_net::gossip::{GossipConfig, GossipError};
use p2panda_net::iroh_endpoint::{EndpointError, IrohConfig, RelayUrl};
use p2panda_net::iroh_mdns::{MdnsDiscoveryError, MdnsDiscoveryMode};
use p2panda_net::{
    AddressBook, DEFAULT_NETWORK_ID, Discovery, Endpoint, Gossip, MdnsDiscovery, NetworkId, NodeId,
};
use thiserror::Error;

#[derive(Clone)]
pub struct Network {
    pub address_book: AddressBook,
    pub mdns: MdnsDiscovery,
    pub endpoint: Endpoint,
    pub discovery: Discovery,
    pub gossip: Gossip,
}

impl Network {
    pub async fn spawn(
        config: NetworkConfig,
        private_key: PrivateKey,
    ) -> Result<Self, NetworkError> {
        // TODO: Pass in store.

        // TODO: Supervision of actors.

        let address_book = AddressBook::builder()
            // TODO: Move address book store into p2panda-store-next
            // .store(address_book_store)
            .spawn()
            .await?;

        for bootstrap in &config.bootstraps {
            address_book.insert_node_info(NodeInfo::new(*bootstrap).bootstrap());
        }

        let mut endpoint = Endpoint::builder(address_book.clone())
            .config(config.iroh)
            .private_key(private_key)
            .network_id(config.network_id);

        for url in &config.relay_urls {
            endpoint = endpoint.relay_url(url.clone());
        }

        let endpoint = endpoint.spawn().await?;

        let mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
            .mode(config.mdns_mode)
            .spawn()
            .await?;

        let discovery = Discovery::builder(address_book.clone(), endpoint.clone())
            .config(config.discovery)
            .spawn()
            .await?;

        let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
            .config(config.gossip)
            .spawn()
            .await?;

        // TODO: Add log sync with topic map and stores.

        Ok(Self {
            address_book,
            endpoint,
            mdns,
            discovery,
            gossip,
        })
    }

    pub fn id(&self) -> NodeId {
        self.endpoint.node_id()
    }

    pub fn network_id(&self) -> NetworkId {
        self.endpoint.network_id()
    }

    pub async fn insert_bootstrap(&self, node_id: NodeId) -> Result<(), NetworkError> {
        let node_info = NodeInfo::new(node_id).bootstrap();
        self.address_book.insert_node_info(node_info).await?;
        Ok(())
    }

    // TODO: Do we need methods to get the transport info (with ip addresses etc.)?
}

#[derive(Clone, Debug)]
pub struct NetworkConfig {
    pub network_id: NetworkId,
    pub relay_urls: HashSet<RelayUrl>,
    pub bootstraps: HashSet<NodeId>,
    pub mdns_mode: MdnsDiscoveryMode,
    pub discovery: DiscoveryConfig,
    pub gossip: GossipConfig,
    pub iroh: IrohConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            network_id: DEFAULT_NETWORK_ID,
            relay_urls: HashSet::new(),
            bootstraps: HashSet::new(),
            mdns_mode: MdnsDiscoveryMode::Active,
            discovery: DiscoveryConfig::default(),
            gossip: GossipConfig::default(),
            iroh: IrohConfig::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    AddressBook(#[from] AddressBookError),

    #[error(transparent)]
    Endpoint(#[from] EndpointError),

    #[error(transparent)]
    Mdns(#[from] MdnsDiscoveryError),

    #[error(transparent)]
    Discovery(#[from] DiscoveryError),

    #[error(transparent)]
    Gossip(#[from] GossipError),
}
