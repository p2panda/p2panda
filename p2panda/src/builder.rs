// SPDX-License-Identifier: MIT OR Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

use p2panda_net::addrs::TrustedTransportInfo;
use p2panda_net::discovery::DiscoveryConfig;
use p2panda_net::gossip::GossipConfig;
use p2panda_net::iroh_endpoint::{EndpointAddr, RelayUrl};
use p2panda_net::utils::from_verifying_key;
use p2panda_net::{NetworkId, NodeId};
use p2panda_store::SqliteStore;
use p2panda_store::sqlite::{SqlitePool, SqliteStoreBuilder};

use crate::network::MdnsDiscoveryMode;
use crate::node::{AckPolicy, Config, SpawnError};
use crate::{Credentials, Node};

/// Builder for `Node`.
///
/// To create the `Node` call `NodeBuilder::spawn()`.
#[derive(Default)]
pub struct NodeBuilder {
    credentials: Option<Credentials>,
    config: Config,
    store_options: StoreBuilderOptions,
}

impl NodeBuilder {
    /// Creates a new `NodeBuilder` using default configuration values.
    pub fn new() -> Self {
        NodeBuilder {
            credentials: None,
            config: Config::default(),
            store_options: StoreBuilderOptions::default(),
        }
    }

    /// Sets the credentials consisting of a signing- and identity secret.
    ///
    /// The public key derived from the given private signing key is used to identify the node in
    /// the network. For example, this key can be used to directly connect to the node. The private
    /// key serves as the means of authenticating the node during the connection handshake (using
    /// TLS 1.3) and is also used to sign operations to ensure data integrity and authenticity.
    ///
    /// The identity secret is used for initial key agreement when encrypting data towards a space
    /// of members.
    ///
    /// If left unset, a new set of credentials will be randomly generated.
    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// Defines the database URL to be used by the store.
    ///
    /// The given URL must take the form of a SQLite database URI: <https://sqlite.org/uri.html>.
    ///
    /// Database migrations are run automatically. Users should use `NodeBuilder::database_pool()`
    /// together with `sqlx` if manual migration management is required.
    ///
    /// If left unset, the node will default to using an ephemeral in-memory database.
    pub fn database_url(mut self, url: &str) -> Self {
        self.store_options = StoreBuilderOptions::Url(url.to_string());
        self
    }

    /// Defines the connection pool to be used by the store.
    ///
    /// If left unset, a new connection pool will be created using the default maximum connections
    /// value of 16 and the database URL (either the default in-memory URL or the URL set by
    /// calling `NodeBuilder::database_url()`).
    pub fn database_pool(mut self, pool: SqlitePool) -> Self {
        self.store_options = StoreBuilderOptions::Pool(pool);
        self
    }

    /// Defines the acknowledgement policy.
    ///
    /// If left unset, the policy defaults to `Automatic` and all messages emitted from topic
    /// streams will be automatically acknowledged.
    ///
    /// See the `Node::stream(topic)` documentation for further information.
    pub fn ack_policy(mut self, value: AckPolicy) -> Self {
        self.config.ack_policy = value;
        self
    }

    /// Sets the network identifier.
    ///
    /// The network identifier is used to achieve separation and prevent interoperability between
    /// distinct networks. This is the most global identifier to group nodes into networks. Different
    /// applications may choose to share the same underlying network infrastructure by using the same
    /// network identifier.
    ///
    /// **WARNING:** The network identifier is _not_ confidentially exchanged with a remote node and
    /// can not be treated as a secret value. See: <https://github.com/p2panda/p2panda/issues/965>
    ///
    /// If left unset, the network ID defaults to the byte representation of the BLAKE3 hash of the
    /// string "p2panda".
    pub fn network_id(mut self, network_id: NetworkId) -> Self {
        self.config.network.network_id = network_id;
        self
    }

    /// Sets a relay server URL to assist in establishing direct connections.
    ///
    /// Multipe relays can be added; a single "home relay" will be automatically selected based on
    /// latency.
    ///
    /// Relays fullfil multiple functions:
    ///
    /// 1. The relay server helps establish connections by temporarily routing encrypted traffic
    ///    until a direct, P2P connection is feasible. This allows nodes to immediately get
    ///    started, without waiting for holepunching / STUN to complete first.
    /// 2. Handle learning a node's public addresses (via QUIC address discovery), signalling and
    ///    hole-punching to establish direct connections between two nodes. This set of methods is
    ///    also understood as STUN. After this point the relay is not required anymore.
    /// 3. Relayed and encrypted fallback using the server when establishing a direct connection
    ///    failed (TURN).
    ///
    /// If no relay is given other nodes can only connect to us if a directly-reachable IP address
    /// is available and known to them.
    pub fn relay_url(mut self, url: RelayUrl) -> Self {
        self.config.network.relay_urls.insert(url);
        self
    }

    /// Inserts a bootstrap node into the local address book.
    ///
    /// Bootstrap nodes are used as a starting point for the random-walk discovery algorithm to
    /// find other nodes in the network, without the need for any centralised registry. Any node
    /// can serve as a bootstrap into the network. The URL of the relay used by the bootstrap node
    /// is required to assist with connectivity (via relaying of traffic and negotiation of
    /// hole-punching for direct connections).
    ///
    /// Multiple bootstrap nodes can be registered. Each iteration of the discovery algorithm
    /// begins by picking a random node from the set of known bootstrap nodes. It's recommended to
    /// register several bootstrap nodes, especially if they are not highly-available; this
    /// offers redunancy in the case that any of the bootstrap nodes go offline or are otherwise
    /// unavailable.
    ///
    /// Consult the documentation of the `p2panda-discovery` crate for further details concerning
    /// the discovery protocol.
    pub fn bootstrap(mut self, node_id: NodeId, relay_url: RelayUrl) -> Self {
        let endpoint_addr =
            EndpointAddr::new(from_verifying_key(node_id)).with_relay_url(relay_url);
        self.config
            .network
            .bootstraps
            .insert((node_id, TrustedTransportInfo::from(endpoint_addr)));
        self
    }

    /// Sets the mDNS discovery mode.
    ///
    /// mDNS may be set to active, passive or disabled mode.
    ///
    /// If left unset, the mode defaults to active and this node will actively advertise it's
    /// endpoint address on the local area network.
    pub fn mdns_mode(mut self, mode: MdnsDiscoveryMode) -> Self {
        self.config.network.mdns_mode = mode;
        self
    }

    /// Binds an IPv4 socket at the given address.
    ///
    /// If left unset, the address defaults to `0.0.0.0`.
    pub fn bind_ip_v4(mut self, ip: Ipv4Addr) -> Self {
        self.config.network.iroh.bind_ip_v4 = ip;
        self
    }

    /// Sets the IPv4 address port.
    ///
    /// If left unset, the port defaults to `0` which results in a random free port being chosen.
    /// If the given port is already in use, a random port will be chosen as a fallback.
    pub fn bind_port_v4(mut self, port: u16) -> Self {
        self.config.network.iroh.bind_port_v4 = port;
        self
    }

    /// Binds an IPv6 socket at the given address.
    ///
    /// If left unset, the address defaults to `[::]`.
    pub fn bind_ip_v6(mut self, ip: Ipv6Addr) -> Self {
        self.config.network.iroh.bind_ip_v6 = ip;
        self
    }

    /// Sets the IPv6 address port.
    ///
    /// If left unset, the port defaults to `0` which results in a random free port being chosen.
    /// If the given port is already in use, a random port will be chosen as a fallback.
    pub fn bind_port_v6(mut self, port: u16) -> Self {
        self.config.network.iroh.bind_port_v6 = port;
        self
    }

    /// Defines custom discovery configuration parameters.
    ///
    /// This allows fine-tuning of the random walk protocol, including the number of walkers and
    /// their reset probability.
    pub fn discovery_config(mut self, config: DiscoveryConfig) -> Self {
        self.config.network.discovery = config;
        self
    }

    /// Defines custom gossip configuration parameters.
    ///
    /// This allows fine-tuning of swarm membership and gossip broadcast parameters, as well as the
    /// maximum message size for broadcast. The default maximum message size is 4096 bytes.
    pub fn gossip_config(mut self, config: GossipConfig) -> Self {
        self.config.network.gossip = config;
        self
    }

    /// Spawns the `Node`.
    pub async fn spawn(self) -> Result<Node, SpawnError> {
        let credentials = self.credentials.unwrap_or_default();
        let store = match self.store_options {
            StoreBuilderOptions::Memory => SqliteStoreBuilder::new().build().await?,
            StoreBuilderOptions::Url(url) => {
                SqliteStoreBuilder::new().database_url(&url).build().await?
            }
            StoreBuilderOptions::Pool(pool) => SqliteStore::from_pool(pool),
        };

        Node::spawn_inner(self.config, store, credentials).await
    }
}

#[derive(Default)]
enum StoreBuilderOptions {
    #[default]
    Memory,
    Url(String),
    Pool(SqlitePool),
}
