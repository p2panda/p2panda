// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_core::{SigningKey, Topic};
use p2panda_store::SqliteStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::address_book::AddressBookError;
use crate::addrs::{NodeInfo, TrustedTransportInfo};
use crate::discovery::DiscoveryError;
use crate::gossip::GossipError;
use crate::iroh_endpoint::{EndpointAddr, EndpointError, RelayUrl};
use crate::iroh_mdns::MdnsDiscoveryError;
use crate::iroh_network::{IrohNetworkConfig, MdnsDiscoveryMode};
use crate::sync::LogSyncError;
use crate::sync::authoriser::SyncBlockList;
use crate::utils;
use crate::{AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NetworkId, NodeId};

/// Fully functional iroh-powered network.
///
/// `IrohNetwork` bundles together a range of peer-to-peer networking functionality.
///
/// It includes an iroh `Endpoint` for initiating and accepting direct network connections over
/// the internet and local area networks; these encrypted connections are relayed when direct
/// peer-to-peer connectivity is not possible (due to problematic NAT traversal setups, for
/// instance).
///
/// An [`AddressBook`] is also included, making it possible to store user-supplied and protocol-derived
/// mappings of node IDs to their transport information.
///
/// Finally, `IrohNetwork` runs several network protocols to aid in peer discovery and data
/// replication.
///
/// The [`Discovery`] protocol uses Private Equality Testing (PET) to discover network peers who
/// share common topics of interest with the local node. `Discovery` also serves as a means of
/// learning the transport addresses of other nodes in the network. `IrohNetwork` includes the
/// option of running the discovery protocol over local area networks using mDNS, or only running
/// it over the wider internet.
///
/// The [`LogSync`] protocol provides an efficient means of catching up on past state and eagerly
/// pushing new data to interested peers; it is the primary means of topic-based data replication
/// currently used by p2panda.
#[allow(unused)]
#[derive(Clone, Debug)]
pub struct IrohNetwork<E>
where
    E: Debug + Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    pub address_book: AddressBook,
    pub mdns: Option<MdnsDiscovery>,
    pub endpoint: Endpoint,
    pub discovery: Discovery,
    pub gossip: Gossip,
    pub log_sync: LogSync<SqliteStore, Topic, E>,
}

impl<E> IrohNetwork<E>
where
    E: Debug + Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    /// Spawns the `IrohNetwork` using the provided configuration.
    ///
    /// The [`SigningKey`] is the private half of a cryptographic keypair. This is used to
    /// establish encrypted connections with network peers. The public half of the keypair serves
    /// as the public identifier of this node (also referred to as the [`NodeId`]).
    ///
    /// An [`IrohNetworkError`] is returned if spawning is unsuccessful due to a network or
    /// store-related failure.
    #[allow(unused)]
    pub async fn spawn(
        config: IrohNetworkConfig,
        signing_key: SigningKey,
        store: SqliteStore,
        sync_block_list: SyncBlockList,
    ) -> Result<Self, IrohNetworkError<E>> {
        let address_book = AddressBook::builder().store(store.clone()).spawn().await?;

        for (node_id, transport_info) in config.bootstraps {
            let mut node_info = NodeInfo::new(node_id).bootstrap();
            if node_info.update_transports(transport_info.into()).is_ok() {
                address_book.insert_node_info(node_info).await?;
            }
        }

        let mut endpoint = Endpoint::builder(address_book.clone())
            .config(config.iroh)
            .signing_key(signing_key)
            .network_id(config.network_id);

        for url in &config.relay_urls {
            endpoint = endpoint.relay_url(url.clone());
        }

        let endpoint = endpoint.spawn().await?;

        let mdns = match config.mdns_mode {
            MdnsDiscoveryMode::Active | MdnsDiscoveryMode::Passive => {
                let mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
                    .mode(config.mdns_mode.into())
                    .spawn()
                    .await?;

                Some(mdns)
            }
            MdnsDiscoveryMode::Disabled => None,
        };

        let discovery = Discovery::builder(address_book.clone(), endpoint.clone())
            .config(config.discovery)
            .spawn()
            .await?;

        let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
            .config(config.gossip)
            .spawn()
            .await?;

        let log_sync = LogSync::builder(store.clone(), endpoint.clone(), gossip.clone())
            .hooks(sync_block_list)
            .spawn()
            .await?;

        Ok(Self {
            address_book,
            endpoint,
            mdns,
            discovery,
            gossip,
            log_sync,
        })
    }

    /// Returns the public identifier of this node.
    #[allow(unused)]
    pub fn id(&self) -> NodeId {
        self.endpoint.node_id()
    }

    /// Returns the network identifier under which this node is running.
    ///
    /// The [`NetworkId`] serves as a network namespace; nodes operating with different network
    /// identifiers do not establish connections or replicate data with one another.
    #[allow(unused)]
    pub fn network_id(&self) -> NetworkId {
        self.endpoint.network_id()
    }

    /// Inserts the contact details of a bootstrap node into the address book.
    ///
    /// A bootstrap node acts as an entry point into the peer-to-peer network, serving as the
    /// the initial contact for running the random-walk discovery protocol. It will introduce us to
    /// one node it knows about; that node then becomes the second step in our random-walk.
    ///
    /// It's important that the node serving as the bootstrap is highly reliable (ie. with a high
    /// level of uptime or availability). If a bootstrap node is known to be less reliable, perhaps
    /// as a result of relying on limited solar-power, consider registering several unique
    /// bootstrap nodes for redundancy.
    ///
    /// Of course, a p2panda network can still operate without a bootstrap node. However, this
    /// requires that node IDs and their transport information be added manually to the
    /// [`AddressBook`].
    #[allow(unused)]
    pub async fn insert_bootstrap(
        &self,
        node_id: NodeId,
        relay_url: RelayUrl,
    ) -> Result<(), IrohNetworkError<E>> {
        let mut node_info = NodeInfo::new(node_id).bootstrap();
        let endpoint_addr =
            EndpointAddr::new(utils::from_verifying_key(node_id)).with_relay_url(relay_url);
        let transport_info = TrustedTransportInfo::from(endpoint_addr);

        if node_info.update_transports(transport_info.into()).is_ok() {
            self.address_book.insert_node_info(node_info).await?;
        }
        Ok(())
    }
}

/// Errors which can occur when spawning or running the iroh networking layer.
#[derive(Debug, Error)]
pub enum IrohNetworkError<E>
where
    E: Debug + Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
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

    #[error(transparent)]
    LogSync(#[from] LogSyncError<E>),
}
