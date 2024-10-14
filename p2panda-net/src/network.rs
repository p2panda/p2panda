// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_lite::StreamExt;
use futures_util::future::{MapErr, Shared};
use futures_util::{FutureExt, TryFutureExt};
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use iroh_gossip::proto::Config as GossipConfig;
use iroh_net::endpoint::TransportConfig;
use iroh_net::key::SecretKey;
use iroh_net::relay::{RelayMap, RelayNode};
use iroh_net::{Endpoint, NodeAddr, NodeId};
use p2panda_core::{PrivateKey, PublicKey};
use p2panda_sync::traits::SyncProtocol;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error, error_span, warn, Instrument};

use crate::addrs::DEFAULT_STUN_PORT;
use crate::config::{Config, DEFAULT_BIND_PORT};
use crate::discovery::{Discovery, DiscoveryMap};
use crate::engine::Engine;
use crate::protocols::{ProtocolHandler, ProtocolMap};
use crate::sync::SYNC_CONNECTION_ALPN;
use crate::{JoinErrToStr, NetworkId, RelayUrl, TopicId};

/// Maximum number of streams accepted on a QUIC connection.
const MAX_STREAMS: u32 = 1024;

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
    sync_protocol: Option<Arc<dyn for<'a> SyncProtocol<'a> + 'static>>,
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
            sync_protocol: None,
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

    /// Sets the sync protocol for this network.
    pub fn sync(mut self, protocol: impl for<'a> SyncProtocol<'a> + 'static) -> Self {
        self.sync_protocol = Some(Arc::new(protocol));
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

            // @TODO: Expose finer-grained config options. Right now we only provide the option of
            // defining the IPv4 port; everything else is hard-coded.
            let bind_port = self.bind_port.unwrap_or(DEFAULT_BIND_PORT);
            let socket_address_v4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port);
            let socket_address_v6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, bind_port + 1, 0, 0);

            Endpoint::builder()
                .transport_config(transport_config)
                .secret_key(secret_key.clone())
                .relay_mode(relay_mode)
                .bind_addr_v4(socket_address_v4)
                .bind_addr_v6(socket_address_v6)
                .bind()
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
            self.sync_protocol,
        );

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

        let sync_handler = engine.sync_handler();

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
        self.protocols.insert(GOSSIP_ALPN, Arc::new(gossip.clone()));
        // If a sync protocol has not been configured then sync handler is None
        if let Some(sync_handler) = sync_handler {
            self.protocols
                .insert(SYNC_CONNECTION_ALPN, Arc::new(sync_handler));
        };
        let protocols = Arc::new(self.protocols.clone());
        let alpns = self.protocols.alpns();
        if let Err(err) = inner.endpoint.set_alpns(alpns) {
            inner.shutdown(protocols.clone()).await;
            return Err(err);
        }

        // Create and spawn network task in runtime
        let fut = inner
            .clone()
            .spawn(protocols.clone())
            .instrument(error_span!("node", me=%node_addr.node_id.fmt_short()));
        let task = tokio::task::spawn(fut);
        let task_handle = AbortOnDropHandle::new(task)
            .map_err(Box::new(|e: JoinError| e.to_string()) as JoinErrToStr)
            .shared();

        let network = Network {
            inner,
            task: task_handle,
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
                Some(incoming) = self.endpoint.accept() => {
                    // @TODO: This is the point at which we can reject the connection if
                    // limits have been reached.
                    let connecting = match incoming.accept() {
                        Ok(connecting) => connecting,
                        Err(err) => {
                            warn!("incoming connection failed: {err:#}");
                            // This may be caused by retransmitted datagrams so we continue.
                            continue;
                        },
                    };
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
        debug!("close all connections and shutdown the node");
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

/// Controls a p2panda-net node, including handling of connections, discovery and gossip.
// @TODO: Go into more detail about the network capabilities and API (usage recommendations etc.)
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Network {
    inner: Arc<NetworkInner>,
    protocols: Arc<ProtocolMap>,
    // `Network` needs to be `Clone + Send` and we need to `task.await` in its `shutdown()` impl.
    // - `Shared` allows us to `task.await` from all `Network` clones
    //   - Acts like an `Arc` around the inner future
    // - `MapErr` is needed to map the `JoinError` to a `String`, since `JoinError` is `!Clone`
    // - `AbortOnDropHandle` ensures the `task` is cancelled when all `Network`s are dropped
    task: Shared<MapErr<AbortOnDropHandle<()>, JoinErrToStr>>,
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

    /// Subscribes to a topic and returns a bi-directional stream that can be read from and
    /// written to.
    ///
    /// Peers subscribed to a topic can be discovered by others via the gossiping overlay ("neighbor
    /// up event"). They'll sync data initially and then start "live" mode via gossip broadcast.
    pub async fn subscribe(
        &self,
        topic: TopicId,
    ) -> Result<(mpsc::Sender<ToNetwork>, broadcast::Receiver<FromNetwork>)> {
        let (to_gossip_tx, to_gossip_rx) = mpsc::channel::<ToNetwork>(128);
        let (from_gossip_tx, from_gossip_rx) = broadcast::channel::<FromNetwork>(128);
        self.inner
            .engine
            .subscribe(topic, from_gossip_tx, to_gossip_rx)
            .await?;
        Ok((to_gossip_tx, from_gossip_rx))
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
/// An event to be broadcast to the network.
pub enum ToNetwork {
    Message { bytes: Vec<u8> },
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
/// An event received from the network.
pub enum FromNetwork {
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
mod sync_protocols {
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_lite::{AsyncRead, AsyncWrite, StreamExt};
    use futures_util::{Sink, SinkExt};
    use p2panda_sync::protocols::utils::{into_sink, into_stream};
    use p2panda_sync::traits::SyncProtocol;
    use p2panda_sync::{FromSync, SyncError};
    use serde::{Deserialize, Serialize};
    use tracing::debug;

    use crate::TopicId;

    #[derive(Debug, Serialize, Deserialize)]
    enum DummyProtocolMessage {
        Topic(TopicId),
        Done,
    }

    /// A sync implementation which fulfills basic protocol requirements but nothing more  
    #[derive(Debug)]
    pub struct DummyProtocol {}

    #[async_trait]
    impl<'a> SyncProtocol<'a> for DummyProtocol {
        fn name(&self) -> &'static str {
            static DUMMY_PROTOCOL_NAME: &str = "dummy_protocol";
            DUMMY_PROTOCOL_NAME
        }
        async fn open(
            self: Arc<Self>,
            topic: &TopicId,
            tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
            rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
            mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
        ) -> Result<(), SyncError> {
            debug!("DummyProtocol: open sync session");

            let mut sink = into_sink(tx);
            let mut stream = into_stream(rx);

            sink.send(DummyProtocolMessage::Topic(*topic)).await?;
            sink.send(DummyProtocolMessage::Done).await?;
            app_tx.send(FromSync::Topic(*topic)).await?;

            while let Some(result) = stream.next().await {
                let message: DummyProtocolMessage = result?;
                debug!("message received: {:?}", message);

                match &message {
                    DummyProtocolMessage::Topic(_) => panic!(),
                    DummyProtocolMessage::Done => break,
                }
            }

            sink.flush().await?;
            app_tx.flush().await?;

            Ok(())
        }

        async fn accept(
            self: Arc<Self>,
            tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
            rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
            mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
        ) -> Result<(), SyncError> {
            debug!("DummyProtocol: accept sync session");

            let mut sink = into_sink(tx);
            let mut stream = into_stream(rx);

            while let Some(result) = stream.next().await {
                let message: DummyProtocolMessage = result?;
                debug!("message received: {:?}", message);

                match &message {
                    DummyProtocolMessage::Topic(topic) => {
                        app_tx.send(FromSync::Topic(*topic)).await?
                    }
                    DummyProtocolMessage::Done => break,
                }
            }

            sink.send(DummyProtocolMessage::Done).await?;

            sink.flush().await?;
            app_tx.flush().await?;

            Ok(())
        }
    }

    // The protocol message types.
    #[derive(Serialize, Deserialize)]
    enum Message {
        Topic(TopicId),
        Ping,
        Pong,
    }

    #[derive(Debug, Clone)]
    pub struct PingPongProtocol {}

    /// A ping-pong sync protocol
    #[async_trait]
    impl<'a> SyncProtocol<'a> for PingPongProtocol {
        fn name(&self) -> &'static str {
            static SIMPLE_PROTOCOL_NAME: &str = "simple_protocol";
            SIMPLE_PROTOCOL_NAME
        }

        async fn open(
            self: Arc<Self>,
            topic: &TopicId,
            tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
            rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
            mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
        ) -> Result<(), SyncError> {
            debug!("open sync session");
            let mut sink = into_sink(tx);
            let mut stream = into_stream(rx);

            sink.send(Message::Topic(*topic)).await?;
            sink.send(Message::Ping).await?;
            debug!("ping message sent");

            app_tx.send(FromSync::Topic(*topic)).await?;

            while let Some(result) = stream.next().await {
                let message = result?;

                match message {
                    Message::Topic(_) => panic!(),
                    Message::Ping => {
                        return Err(SyncError::Protocol(
                            "unexpected Ping message received".to_string(),
                        ));
                    }
                    Message::Pong => {
                        debug!("pong message received");
                        break;
                    }
                }
            }

            // Flush all bytes so that no messages are lost.
            sink.flush().await?;
            app_tx.flush().await?;

            Ok(())
        }

        async fn accept(
            self: Arc<Self>,
            tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
            rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
            mut app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
        ) -> Result<(), SyncError> {
            debug!("accept sync session");
            let mut sink = into_sink(tx);
            let mut stream = into_stream(rx);

            while let Some(result) = stream.next().await {
                let message = result?;

                match message {
                    Message::Topic(topic) => app_tx.send(FromSync::Topic(topic)).await?,
                    Message::Ping => {
                        debug!("ping message received");
                        sink.send(Message::Pong).await?;
                        debug!("pong message sent");
                        break;
                    }
                    Message::Pong => {
                        return Err(SyncError::Protocol(
                            "unexpected Pong message received".to_string(),
                        ));
                    }
                }
            }

            sink.flush().await?;
            app_tx.flush().await?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Duration;

    use iroh_net::relay::{RelayNode, RelayUrl as IrohRelayUrl};
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore, TopicMap};
    use p2panda_sync::protocols::log_height::LogHeightSyncProtocol;
    use serde::Serialize;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    use crate::addrs::DEFAULT_STUN_PORT;
    use crate::config::Config;
    use crate::network::sync_protocols::PingPongProtocol;
    use crate::{NetworkBuilder, RelayMode, RelayUrl, ToBytes, TopicId};

    use super::{FromNetwork, ToNetwork};

    fn setup_logging() {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(EnvFilter::from_default_env())
            .try_init()
            .ok();
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
        setup_logging();

        let network_id = [1; 32];

        let node_1 = NetworkBuilder::new(network_id).build().await.unwrap();
        let node_2 = NetworkBuilder::new(network_id).build().await.unwrap();

        let node_1_addr = node_1.endpoint().node_addr().await.unwrap();
        let node_2_addr = node_2.endpoint().node_addr().await.unwrap();

        node_1.add_peer(node_2_addr).await.unwrap();
        node_2.add_peer(node_1_addr).await.unwrap();

        // Subscribe to the same topic from both nodes
        let (tx_1, mut rx_1) = node_1.subscribe([0; 32]).await.unwrap();
        let (_tx_2, mut rx_2) = node_2.subscribe([0; 32]).await.unwrap();

        // Receive the first message for both nodes
        let rx_2_msg = rx_2.recv().await.unwrap();
        let rx_1_msg = rx_1.recv().await.unwrap();

        // Ensure the gossip-overlay has been joined for the given topic
        assert!(matches!(rx_1_msg, FromNetwork::Ready));
        assert!(matches!(rx_2_msg, FromNetwork::Ready));

        // Broadcast a message and make sure it's received by the other node
        tx_1.send(ToNetwork::Message {
            bytes: "Hello, Node".to_bytes(),
        })
        .await
        .unwrap();

        let rx_2_msg = rx_2.recv().await.unwrap();
        assert_eq!(
            rx_2_msg,
            FromNetwork::Message {
                bytes: "Hello, Node".to_bytes(),
                delivered_from: node_1.node_id(),
            }
        );

        println!("shutdown nodes");
        node_1.shutdown().await.unwrap();
        node_2.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn ping_pong() {
        setup_logging();

        let network_id = [1; 32];
        let topic_id = [0; 32];

        let ping_pong = PingPongProtocol {};

        let node_1 = NetworkBuilder::new(network_id)
            .sync(ping_pong.clone())
            .build()
            .await
            .unwrap();
        let node_2 = NetworkBuilder::new(network_id)
            .sync(ping_pong)
            .build()
            .await
            .unwrap();

        let node_1_addr = node_1.endpoint().node_addr().await.unwrap();
        let node_2_addr = node_2.endpoint().node_addr().await.unwrap();

        node_1.add_peer(node_2_addr).await.unwrap();
        node_2.add_peer(node_1_addr).await.unwrap();

        // Subscribe to the same topic from both nodes which should kick off sync
        let handle1 = tokio::spawn(async move {
            let (_tx, _rx) = node_1.subscribe(topic_id).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
            node_1.shutdown().await.unwrap();
        });
        let handle2 = tokio::spawn(async move {
            let (_tx, _rx) = node_2.subscribe(topic_id).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
            node_2.shutdown().await.unwrap();
        });

        let (result1, result2) = tokio::join!(handle1, handle2);
        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    /// Helper method for generating p2panda operations
    fn generate_operation<E: Clone + Serialize>(
        private_key: &PrivateKey,
        body: Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
        extensions: Option<E>,
    ) -> Operation<E> {
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp,
            seq_num,
            backlink,
            previous: vec![],
            extensions,
        };
        header.sign(&private_key);

        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }

    #[derive(Clone, Debug)]
    struct LogIdTopicMap(HashMap<TopicId, String>);

    impl LogIdTopicMap {
        pub fn new() -> Self {
            LogIdTopicMap(HashMap::new())
        }
    }

    impl TopicMap<TopicId, String> for LogIdTopicMap {
        fn get(&self, topic: &TopicId) -> Option<String> {
            self.0.get(topic).cloned()
        }
    }

    impl LogIdTopicMap {
        pub fn insert(&mut self, topic: TopicId, scope: String) -> Option<String> {
            self.0.insert(topic, scope)
        }
    }

    #[tokio::test]
    async fn e2e_log_height_sync() {
        setup_logging();

        const NETWORK_ID: [u8; 32] = [1; 32];
        const TOPIC_ID: [u8; 32] = [0u8; 32];
        let log_id = String::from("messages");

        let peer_a_private_key = PrivateKey::new();
        let peer_b_private_key = PrivateKey::new();

        // Construct a store and log height protocol for peer a
        let store_a = MemoryStore::default();
        let mut topic_map = LogIdTopicMap::new();
        topic_map.insert(TOPIC_ID, log_id.clone());
        let protocol_a = LogHeightSyncProtocol {
            topic_map: topic_map.clone(),
            store: store_a,
        };

        // Create some operations
        let body = Body::new("Hello, Sloth!".as_bytes());
        let operation0 = generate_operation(&peer_a_private_key, body.clone(), 0, 0, None, None);
        let operation1 = generate_operation(
            &peer_a_private_key,
            body.clone(),
            1,
            100,
            Some(operation0.hash),
            None,
        );
        let operation2 = generate_operation(
            &peer_a_private_key,
            body.clone(),
            2,
            200,
            Some(operation1.hash),
            None,
        );

        // Create store for peer b and populate with operations
        let mut store_b = MemoryStore::default();
        store_b
            .insert_operation(&operation0, &log_id)
            .await
            .unwrap();
        store_b
            .insert_operation(&operation1, &log_id)
            .await
            .unwrap();
        store_b
            .insert_operation(&operation2, &log_id)
            .await
            .unwrap();

        // Construct log height protocol for peer b
        let protocol_b = LogHeightSyncProtocol {
            topic_map,
            store: store_b,
        };

        // Build peer a's node
        let node_a = NetworkBuilder::new(NETWORK_ID)
            .sync(protocol_a)
            .private_key(peer_a_private_key)
            .build()
            .await
            .unwrap();

        // Build peer b's node
        let node_b = NetworkBuilder::new(NETWORK_ID)
            .sync(protocol_b)
            .private_key(peer_b_private_key.clone())
            .build()
            .await
            .unwrap();

        let node_a_addr = node_a.endpoint().node_addr().await.unwrap();
        let node_b_addr = node_b.endpoint().node_addr().await.unwrap();

        node_a.add_peer(node_b_addr).await.unwrap();
        node_b.add_peer(node_a_addr).await.unwrap();

        // Subscribe to the same topic from both nodes which should kick off sync
        let handle1 = tokio::spawn(async move {
            let (_tx, mut from_sync_rx) = node_a.subscribe(TOPIC_ID).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;

            let mut from_sync_messages = Vec::new();
            while let Ok(message) = from_sync_rx.recv().await {
                from_sync_messages.push(message);
                if from_sync_messages.len() == 4 {
                    break;
                }
            }

            // Construct the messages we expect to receive on the from_sync channel based on the
            // operations we created earlier.
            let mut operation0_bytes = Vec::new();
            ciborium::into_writer(&(operation0.header, operation0.body), &mut operation0_bytes)
                .unwrap();
            let mut operation1_bytes = Vec::new();
            ciborium::into_writer(&(operation1.header, operation1.body), &mut operation1_bytes)
                .unwrap();
            let mut operation2_bytes = Vec::new();
            ciborium::into_writer(&(operation2.header, operation2.body), &mut operation2_bytes)
                .unwrap();

            let peer_a_expected_messages = vec![
                FromNetwork::Ready,
                FromNetwork::Message {
                    bytes: operation0_bytes.clone(),
                    delivered_from: peer_b_private_key.public_key(),
                },
                FromNetwork::Message {
                    bytes: operation1_bytes.clone(),
                    delivered_from: peer_b_private_key.public_key(),
                },
                FromNetwork::Message {
                    bytes: operation2_bytes.clone(),
                    delivered_from: peer_b_private_key.public_key(),
                },
            ];

            // Assert we receive the expected messages
            assert_eq!(from_sync_messages, peer_a_expected_messages);

            node_a.shutdown().await.unwrap();
        });

        let handle2 = tokio::spawn(async move {
            let (_tx, mut from_sync_rx) = node_b.subscribe(TOPIC_ID).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;

            let mut from_sync_messages = Vec::new();
            while let Ok(message) = from_sync_rx.recv().await {
                from_sync_messages.push(message);
                if from_sync_messages.len() == 1 {
                    break;
                }
            }
            // Assert the channel is now empty
            assert!(from_sync_rx.is_empty());
            // Assert we receive the expected messages
            assert_eq!(from_sync_messages, vec![FromNetwork::Ready]);

            node_b.shutdown().await.unwrap();
        });

        // Wait on both to complete
        let (result1, result2) = tokio::join!(handle1, handle2);

        assert!(result1.is_ok());
        assert!(result2.is_ok())
    }
}
