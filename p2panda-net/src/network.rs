// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_lite::StreamExt;
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use iroh_gossip::proto::Config as GossipConfig;
use iroh_net::endpoint::TransportConfig;
use iroh_net::key::SecretKey;
use iroh_net::relay::{RelayMap, RelayNode};
use iroh_net::util::SharedAbortingJoinHandle;
use iroh_net::{Endpoint, NodeAddr, NodeId};
use p2panda_core::{PrivateKey, PublicKey};
use p2panda_sync::traits::SyncProtocol;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, error_span, warn, Instrument};

use crate::addrs::DEFAULT_STUN_PORT;
use crate::config::{Config, DEFAULT_BIND_PORT};
use crate::discovery::{Discovery, DiscoveryMap};
use crate::engine::sync::SyncProtocolMap;
use crate::engine::Engine;
use crate::handshake::{Handshake, HANDSHAKE_ALPN};
use crate::protocols::{ProtocolHandler, ProtocolMap};
use crate::{NetworkId, RelayUrl, TopicId};

/// Maximum number of streams accepted on a QUIC connection.
const MAX_STREAMS: u32 = 1024;

/// Maximum number of parallel QUIC connections.
const MAX_CONNECTIONS: u32 = 1024;

/// Timeout duration for discovery of at least one peer's direct address.
const ENDPOINT_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
pub enum RelayMode {
    Disabled,
    Custom(RelayNode),
}

/// Creates an overlay network for peers grouped under the same network identifier.
///
/// All peers can subscribe to multiple topics in this overlay and hook into a data stream per
/// topic where they'll send and receive data.
#[derive(Debug)]
pub struct NetworkBuilder {
    bind_port: Option<u16>,
    direct_node_addresses: Vec<NodeAddr>,
    discovery: DiscoveryMap,
    gossip_config: Option<GossipConfig>,
    network_id: NetworkId,
    protocols: ProtocolMap,
    sync: SyncProtocolMap,
    relay_mode: RelayMode,
    secret_key: Option<SecretKey>,
}

impl NetworkBuilder {
    /// Returns a new instance of `NetworkBuilder` using the given network identifier.
    ///
    /// The identifier is used during handshake and discovery protocols. Networks must use the
    /// same identifier if they wish to successfully connect and share gossip.
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            bind_port: None,
            direct_node_addresses: Vec::new(),
            discovery: DiscoveryMap::default(),
            gossip_config: None,
            network_id,
            protocols: Default::default(),
            sync: Default::default(),
            relay_mode: RelayMode::Disabled,
            secret_key: None,
        }
    }

    /// Returns a new instance of `NetworkBuilder` using the given configuration.
    pub fn from_config(config: Config) -> Self {
        let mut network_builder = Self::new(config.network_id).bind_port(config.bind_port);

        for (public_key, addresses, relay_addr) in config.direct_node_addresses {
            network_builder = network_builder.direct_address(public_key, addresses, relay_addr)
        }

        if let Some(url) = config.relay {
            let port = url.port().unwrap_or(DEFAULT_STUN_PORT);
            network_builder = network_builder.relay(url, false, port)
        }

        network_builder
    }

    /// Sets or overwrites the the local bind port.
    pub fn bind_port(mut self, port: u16) -> Self {
        self.bind_port.replace(port);
        self
    }

    /// Sets or overwrites the private key.
    ///
    /// If this value is not set, the `NetworkBuilder` will generate a new, random key when
    /// building the network.
    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.secret_key = Some(SecretKey::from_bytes(private_key.as_bytes()));
        self
    }

    /// Sets the relay used by the local network to facilitate the establishment of direct
    /// connections.
    ///
    /// Relay nodes are STUN servers to help establishing a peer-to-peer connection if either or
    /// both of the peers are behind a NAT. If this connection attempt fails, the relay node might
    /// offer a proxy functionality on top, which will help to relay the data in that case.
    pub fn relay(mut self, url: RelayUrl, stun_only: bool, stun_port: u16) -> Self {
        self.relay_mode = RelayMode::Custom(RelayNode {
            url: url.into(),
            stun_only,
            stun_port,
        });
        self
    }

    /// Sets the direct address of a peer, identified by their public key (node id).
    ///
    /// If given a direct address, it should be reachable without the aid of a STUN / relay node.
    /// If the direct connection attempt fails (for example because of a NAT or Firewall) the relay
    /// node of that peer needs to be given, so we can re-attempt establishing a connection with it.
    ///
    /// If no relay address is given but turns out to be required, we optimistically try to use our
    /// own relay node instead (if specified). This might still fail, as we can't know if the peer
    /// is using the same relay node.
    pub fn direct_address(
        mut self,
        node_id: PublicKey,
        addresses: Vec<SocketAddr>,
        relay_addr: Option<RelayUrl>,
    ) -> Self {
        let node_id = NodeId::from_bytes(node_id.as_bytes()).expect("invalid public key");
        let mut node_addr = NodeAddr::new(node_id).with_direct_addresses(addresses);
        if let Some(url) = relay_addr {
            node_addr = node_addr.with_relay_url(url.into());
        }
        self.direct_node_addresses.push(node_addr);
        self
    }

    /// Adds one or more discovery strategy, such as mDNS.
    pub fn discovery(mut self, handler: impl Discovery + 'static) -> Self {
        self.discovery.add(handler);
        self
    }

    /// Adds a sync handler an to existing gossip topics.
    pub fn sync(mut self, topic: TopicId, handler: impl SyncProtocol + 'static) -> Self {
        self.sync.add(topic.into(), handler);
        self
    }

    /// Sets the gossip configuration.
    ///
    /// Configuration parameters define the behavior of the swarm membership (HyParView) and gossip
    /// broadcast (Plumtree) layers, as well as the maximum message size.
    pub fn gossip(mut self, config: GossipConfig) -> Self {
        self.gossip_config = Some(config);
        self
    }

    /// Adds protocols for network communication.
    pub fn protocol(
        mut self,
        protocol_name: &'static [u8],
        handler: impl ProtocolHandler + 'static,
    ) -> Self {
        self.protocols.insert(protocol_name, Arc::new(handler));
        self
    }

    /// Returns a handle to a newly-spawned instance of `Network`.
    ///
    /// A peer-to-peer endpoint is created and bound to a QUIC socket, after which the gossip,
    /// engine and handshake handlers are instantiated. Direct addresses for network peers are
    /// added to the engine from the address book and core protocols are registered.
    ///
    /// After configuration and registration processes are complete, the network is spawned and an
    /// attempt is made to retrieve a direct address for a network peer so that a connection
    /// attempt may be made. If no address is retrieved within the timeout limit, the network is
    /// shut down and an error is returned.
    pub async fn build(mut self) -> Result<Network> {
        let secret_key = self.secret_key.unwrap_or(SecretKey::generate());

        let relay: Option<RelayNode> = match self.relay_mode {
            RelayMode::Disabled => None,
            RelayMode::Custom(ref node) => Some(node.clone()),
        };

        // Build p2p endpoint and bind the QUIC socket
        let endpoint = {
            let mut transport_config = TransportConfig::default();
            transport_config
                .max_concurrent_bidi_streams(MAX_STREAMS.into())
                .max_concurrent_uni_streams(0u32.into());

            let relay_mode = match self.relay_mode {
                RelayMode::Disabled => iroh_net::relay::RelayMode::Disabled,
                RelayMode::Custom(node) => iroh_net::relay::RelayMode::Custom(
                    RelayMap::from_nodes(vec![node])
                        .expect("relay list can not contain duplicates"),
                ),
            };

            Endpoint::builder()
                .transport_config(transport_config)
                .secret_key(secret_key.clone())
                .relay_mode(relay_mode)
                .concurrent_connections(MAX_CONNECTIONS)
                .bind(self.bind_port.unwrap_or(DEFAULT_BIND_PORT))
                .await?
        };

        let node_addr = endpoint.node_addr().await?;

        // Set up gossip overlay handler
        let gossip = Gossip::from_endpoint(
            endpoint.clone(),
            self.gossip_config.unwrap_or_default(),
            &node_addr.info,
        );

        let engine = Engine::new(
            self.network_id,
            endpoint.clone(),
            gossip.clone(),
            self.sync.clone(),
        );
        let handshake = Handshake::new(gossip.clone());

        // Add direct addresses to address book
        for mut direct_addr in self.direct_node_addresses {
            if direct_addr.relay_url().is_none() {
                // If given address does not hold any relay information we optimistically add ours
                // (if we have one). It's not guaranteed that this address will have the same relay
                // url as we have, but it's better than nothing!
                if let Some(ref relay_node) = relay {
                    direct_addr = direct_addr.with_relay_url(relay_node.url.clone());
                }
            }

            engine.add_peer(direct_addr.clone()).await?;
        }

        let inner = Arc::new(NetworkInner {
            cancel_token: CancellationToken::new(),
            relay,
            discovery: self.discovery,
            endpoint: endpoint.clone(),
            engine,
            gossip: gossip.clone(),
            network_id: self.network_id,
            secret_key,
        });

        // Register core protocols all nodes accept
        self.protocols.insert(GOSSIP_ALPN, Arc::new(gossip));
        self.protocols.insert(HANDSHAKE_ALPN, Arc::new(handshake));
        let protocols = Arc::new(self.protocols);
        if let Err(err) = inner.endpoint.set_alpns(protocols.alpns()) {
            inner.shutdown(protocols).await;
            return Err(err);
        }

        // Create and spawn network task in runtime
        let fut = inner
            .clone()
            .spawn(protocols.clone())
            .instrument(error_span!("node", me=%node_addr.node_id.fmt_short()));
        let task = tokio::task::spawn(fut);

        let network = Network {
            inner,
            task: task.into(),
            protocols,
        };

        // Wait for a single direct address update, to make sure we found at least one direct
        // address
        let wait_for_endpoints = {
            async move {
                tokio::time::timeout(ENDPOINT_WAIT, endpoint.direct_addresses().next())
                    .await
                    .context("waiting for endpoint")?
                    .context("no endpoints given to establish at least one connection")?;
                Ok(())
            }
        };

        if let Err(err) = wait_for_endpoints.await {
            network.shutdown().await.ok();
            return Err(err);
        }

        Ok(network)
    }
}

/// Controls a p2panda-net node, including handling of connections, discovery and gossip.
// @TODO: Go into more detail about the network capabilities and API (usage recommendations etc.)
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Network {
    inner: Arc<NetworkInner>,
    protocols: Arc<ProtocolMap>,
    task: SharedAbortingJoinHandle<()>,
}

#[allow(dead_code)]
#[derive(Debug)]
struct NetworkInner {
    cancel_token: CancellationToken,
    relay: Option<RelayNode>,
    discovery: DiscoveryMap,
    endpoint: Endpoint,
    engine: Engine,
    gossip: Gossip,
    network_id: NetworkId,
    secret_key: SecretKey,
}

/// Spawns a network.
///
/// Local network sockets are bound and a task is started to listen for direct addresses changes
/// for the local endpoint. Inbound connection attempts to these endpoints are passed to a handler.
///
/// Any registered discovery services are subscribed to so that the identifiers and addresses of
/// peers operating on the same network may be learned. Discovered peers are added to the local
/// address book so they may be involved in connection and gossip activites.
impl NetworkInner {
    async fn spawn(self: Arc<Self>, protocols: Arc<ProtocolMap>) {
        let (ipv4, ipv6) = self.endpoint.bound_sockets();
        debug!(
            "listening at: {}{}",
            ipv4,
            ipv6.map(|addr| format!(" and {addr}")).unwrap_or_default()
        );

        let mut join_set = JoinSet::<Result<()>>::new();

        // Spawn a task that updates the gossip endpoints and discovery services
        {
            let inner = self.clone();
            join_set.spawn(async move {
                let mut addrs_stream = inner.endpoint.direct_addresses();
                let mut my_node_addr = NodeAddr::new(inner.endpoint.node_id());
                if let Some(node) = &inner.relay {
                    my_node_addr = my_node_addr.with_relay_url(node.url.to_owned());
                }

                loop {
                    tokio::select! {
                        // Learn about our direct addresses and changes to them
                        Some(endpoints) = addrs_stream.next() => {
                            if let Err(err) = inner.gossip.update_direct_addresses(&endpoints) {
                                warn!("Failed to update direct addresses for gossip: {err:?}");
                            }

                            let direct_addresses = endpoints.iter().map(|endpoint| endpoint.addr).collect();
                            my_node_addr.info.direct_addresses = direct_addresses;
                            if let Err(err) = inner.discovery.update_local_address(&my_node_addr) {
                                warn!("Failed to update direct addresses for discovery: {err:?}");
                            }
                        },
                        else => break,
                    }
                }

                Ok(())
            });
        }

        // Subscribe to all discovery channels where we might find new peers
        let mut discovery_stream = self
            .discovery
            .subscribe(self.network_id)
            .expect("discovery map needs to be given");

        loop {
            tokio::select! {
                // Do not let tokio select futures randomly but with top-to-bottom priority
                biased;
                // Exit loop when shutdown was signalled somewhere else
                _ = self.cancel_token.cancelled() => {
                    break;
                },
                // Handle incoming p2p connections
                Some(connecting) = self.endpoint.accept() => {
                    let protocols = protocols.clone();
                    join_set.spawn(async move {
                        handle_connection(connecting, protocols).await;
                        Ok(())
                    });
                },
                // Handle discovered peers
                Some(event) = discovery_stream.next() => {
                    match event {
                        Ok(event) => {
                            if let Err(err) = self.engine.add_peer(event.node_addr).await {
                                error!("Engine failed on add_peer: {err:?}");
                                break;
                            }
                        }
                        Err(err) => {
                            error!("Discovery service failed: {err:?}");
                            break;
                        },
                    }
                },
                // Handle task terminations and quit on panics
                res = join_set.join_next(), if !join_set.is_empty() => {
                    match res {
                        Some(Err(outer)) => {
                            if outer.is_panic() {
                                error!("Task panicked: {outer:?}");
                                break;
                            } else if outer.is_cancelled() {
                                debug!("Task cancelled: {outer:?}");
                            } else {
                                error!("Task failed: {outer:?}");
                                break;
                            }
                        }
                        Some(Ok(Err(inner))) => {
                            debug!("Task errored: {inner:?}");
                        }
                        _ => {}
                    }
                },
                else => break,
            }
        }

        self.shutdown(protocols).await;

        // Abort remaining tasks
        join_set.shutdown().await;
    }

    /// Closes all connections and shuts down the network engine.
    async fn shutdown(&self, protocols: Arc<ProtocolMap>) {
        // We ignore all errors during shutdown
        let _ = tokio::join!(
            // Close the endpoint. Closing the Endpoint is the equivalent of calling
            // Connection::close on all connections: Operations will immediately fail with
            // ConnectionError::LocallyClosed. All streams are interrupted, this is not graceful
            self.endpoint
                .clone()
                .close(1u32.into(), b"provider terminating"),
            // Shutdown engine
            self.engine.shutdown(),
            // Shutdown protocol handlers
            protocols.shutdown(),
        );
    }
}

impl Network {
    /// Returns the public key of the local network.
    pub fn node_id(&self) -> PublicKey {
        PublicKey::from_bytes(self.inner.endpoint.node_id().as_bytes())
            .expect("public key already checked")
    }

    /// Returns the direct addresses of the local network.
    pub async fn direct_addresses(&self) -> Option<Vec<SocketAddr>> {
        self.inner
            .endpoint
            .direct_addresses()
            .next()
            .await
            .map(|addrs| addrs.into_iter().map(|direct| direct.addr).collect())
    }

    /// Subscribes to a topic and returns a bi-directional stream from which can be read from
    /// and written to.
    ///
    /// Peers subscribed to a topic can be discovered by others via the gossiping overlay ("neighbor
    /// up event"). They'll sync data initially (when a sync protocol is given) and then start
    /// "live" mode via gossip broadcast.
    pub async fn subscribe(
        &self,
        topic: TopicId,
        sync: impl SyncProtocol + 'static,
    ) -> Result<(mpsc::Sender<InEvent>, broadcast::Receiver<OutEvent>)> {
        let (in_tx, in_rx) = mpsc::channel::<InEvent>(128);
        let (out_tx, out_rx) = broadcast::channel::<OutEvent>(128);
        self.inner
            .engine
            .subscribe(topic, sync, out_tx, in_rx)
            .await?;
        Ok((in_tx, out_rx))
    }

    /// Returns a handle to the network endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.inner.endpoint
    }

    /// Adds a peer to the local network address book.
    pub async fn add_peer(&self, node_addr: NodeAddr) -> Result<()> {
        self.inner.engine.add_peer(node_addr).await
    }

    /// Returns the addresses of all known peers.
    pub async fn known_peers(&self) -> Result<Vec<NodeAddr>> {
        self.inner.engine.known_peers().await
    }

    /// Terminates the main network task and shuts down the network.
    pub async fn shutdown(self) -> Result<()> {
        // Trigger shutdown of the main run task by activating the cancel token
        self.inner.cancel_token.cancel();

        // Wait for the main task to terminate
        self.task.await.map_err(|err| anyhow!(err))?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
/// An event to be broadcast to the gossip-overlay.
pub enum InEvent {
    Message { bytes: Vec<u8> },
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
/// An event received from the gossip-overlay.
// @TODO: Maybe consider renaming these two enums...
// Could be switched to OutboundEvent and InboundEvent (in relation to the gossip-overlay).
pub enum OutEvent {
    Ready,
    Message {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    },
}

/// Handle an inbound connection on the local network endpoint.
///
/// The connection is accepted if the handshake is successful and the peer is operating with
/// a supported ALPN protocol.
async fn handle_connection(
    mut connecting: iroh_net::endpoint::Connecting,
    protocols: Arc<ProtocolMap>,
) {
    let alpn = match connecting.alpn().await {
        Ok(alpn) => alpn,
        Err(err) => {
            warn!("ignoring connection: invalid handshake: {:?}", err);
            return;
        }
    };
    let Some(handler) = protocols.get(&alpn) else {
        warn!("ignoring connection: unsupported alpn protocol");
        return;
    };
    if let Err(err) = handler.accept(connecting).await {
        warn!("handling incoming connection ended with error: {err}");
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_lite::{AsyncRead, AsyncWrite};
    use futures_util::Sink;
    use iroh_net::relay::{RelayNode, RelayUrl as IrohRelayUrl};
    use p2panda_core::PrivateKey;
    use p2panda_sync::traits::SyncProtocol;
    use p2panda_sync::{SyncError, TopicId};
    use tracing::debug;

    use crate::addrs::DEFAULT_STUN_PORT;
    use crate::config::Config;
    use crate::{NetworkBuilder, RelayMode, RelayUrl, ToBytes};

    use super::{InEvent, OutEvent};

    pub struct DummyProtocol {}

    #[async_trait]
    impl SyncProtocol for DummyProtocol {
        fn name(&self) -> &'static str {
            static DUMMY_PROTOCOL_NAME: &str = "dummy_protocol";
            DUMMY_PROTOCOL_NAME
        }
        async fn run(
            self: Arc<Self>,
            topic: &TopicId,
            _tx: Box<dyn AsyncWrite + Send + Unpin>,
            _rx: Box<dyn AsyncRead + Send + Unpin>,
            mut _app_tx: Box<dyn Sink<Vec<u8>, Error = SyncError> + Send + Unpin>,
        ) -> Result<(), SyncError> {
            debug!("dummy sync protocol ran");
            Ok(())
        }
    }

    #[tokio::test]
    async fn config() {
        let direct_node_public_key = PrivateKey::new().public_key();
        let relay_address: RelayUrl = "https://example.net".parse().unwrap();

        let config = Config {
            bind_port: 2024,
            network_id: [1; 32],
            private_key: Some(PathBuf::new().join("secret-key.txt")),
            direct_node_addresses: vec![(
                direct_node_public_key,
                vec!["0.0.0.0:2026".parse().unwrap()],
                None,
            )
                .into()],
            relay: Some(relay_address.clone()),
        };

        let builder = NetworkBuilder::from_config(config);

        assert_eq!(builder.bind_port, Some(2024));
        assert_eq!(builder.network_id, [1; 32]);
        assert!(builder.secret_key.is_none());
        assert_eq!(builder.direct_node_addresses.len(), 1);
        let relay_node = RelayNode {
            url: IrohRelayUrl::from(relay_address),
            stun_only: false,
            stun_port: DEFAULT_STUN_PORT,
        };
        assert_eq!(builder.relay_mode, RelayMode::Custom(relay_node));
    }

    #[tokio::test]
    async fn join_gossip_overlay() {
        let network_id = [1; 32];

        let node_1 = NetworkBuilder::new(network_id).build().await.unwrap();
        let node_2 = NetworkBuilder::new(network_id).build().await.unwrap();

        let node_1_addr = node_1.endpoint().node_addr().await.unwrap();
        let node_2_addr = node_2.endpoint().node_addr().await.unwrap();

        node_1.add_peer(node_2_addr).await.unwrap();
        node_2.add_peer(node_1_addr).await.unwrap();

        // Subscribe to the same topic from both nodes
        let (tx_1, mut rx_1) = node_1.subscribe([0; 32], DummyProtocol {}).await.unwrap();
        let (_tx_2, mut rx_2) = node_2.subscribe([0; 32], DummyProtocol {}).await.unwrap();

        // Receive the first message for both nodes
        let rx_2_msg = rx_2.recv().await.unwrap();
        let rx_1_msg = rx_1.recv().await.unwrap();

        // Ensure the gossip-overlay has been joined for the given topic
        assert!(matches!(rx_1_msg, OutEvent::Ready));
        assert!(matches!(rx_2_msg, OutEvent::Ready));

        // Broadcast a message and make sure it's received by the other node
        tx_1.send(InEvent::Message {
            bytes: "Hello, Node".to_bytes(),
        })
        .await
        .unwrap();

        let rx_2_msg = rx_2.recv().await.unwrap();
        assert_eq!(
            rx_2_msg,
            OutEvent::Message {
                bytes: "Hello, Node".to_bytes(),
                delivered_from: node_1.node_id(),
            }
        );

        node_1.shutdown().await.unwrap();
        node_2.shutdown().await.unwrap();
    }
}
