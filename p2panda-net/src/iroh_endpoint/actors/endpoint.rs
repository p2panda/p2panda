// SPDX-License-Identifier: MIT OR Apache-2.0

//! Actor managing an endpoint to establish direct or relayed connections over the Internet
//! Protocol using the "iroh" crate.
use std::collections::BTreeMap;
use std::net::{SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::TransportConfig;
use iroh::protocol::DynProtocolHandler;
use p2panda_core::PrivateKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::address_book::report::{ConnectionOutcome, ConnectionRole};
use crate::address_book::{AddressBook, AddressBookError};
use crate::iroh_endpoint::actors::connection::{
    ConnectReplyPort, ConnectionActorError, IrohConnection, IrohConnectionArgs,
};
use crate::iroh_endpoint::actors::is_globally_reachable_endpoint;
use crate::iroh_endpoint::config::IrohConfig;
use crate::iroh_endpoint::discovery::AddressBookDiscovery;
use crate::iroh_endpoint::from_private_key;
use crate::utils::ShortFormat;
use crate::{NetworkId, NodeId, ProtocolId, hash_protocol_id_with_network_id};

/// Period of inactivity before sending a keep-alive packet.
///
/// Keep-alive packets prevent an inactive but otherwise healthy connection from timing out.
///
/// Must be set lower than the idle_timeout of both peers to be effective.
pub const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum duration of inactivity to accept before timing out the connection.
pub const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

#[allow(clippy::large_enum_variant)]
pub enum ToIrohEndpoint {
    /// Bind the iroh endpoint.
    ///
    /// This takes place automatically after the actor started.
    Bind,

    /// Return the internal iroh endpoint instance (used by iroh-gossip).
    Endpoint(RpcReplyPort<iroh::Endpoint>),

    /// Register protocol handler for a given ALPN (protocol identifier).
    RegisterProtocol(ProtocolId, Box<dyn DynProtocolHandler>),

    /// Starts a connection attempt to a remote iroh endpoint and returns a future which can be
    /// awaited for establishing the final connection.
    ///
    /// The ALPN byte string, or application-level protocol identifier, is also required. The
    /// remote endpoint must support this alpn, otherwise the connection attempt will fail with an
    /// error.
    Connect(
        NodeId,
        ProtocolId,
        Option<Arc<TransportConfig>>,
        ConnectReplyPort,
    ),

    /// We've received a connection attempt from a remote iroh endpoint.
    Incoming(iroh::endpoint::Incoming),

    /// Inform endpoint actor about this successful, incoming connection attempt.
    ///
    /// It will help us to remove any "stale" status of this node since it successfully contacted
    /// us now.
    Report {
        remote_node_id: NodeId,
        role: ConnectionRole,
        outcome: ConnectionOutcome,
    },
}

pub type ProtocolMap = Arc<RwLock<BTreeMap<ProtocolId, Box<dyn DynProtocolHandler>>>>;

pub struct IrohState {
    network_id: NetworkId,
    private_key: PrivateKey,
    config: IrohConfig,
    address_book: AddressBook,
    endpoint: Option<iroh::Endpoint>,
    protocols: ProtocolMap,
    accept_handle: Option<JoinHandle<()>>,
    watch_addr_handle: Option<JoinHandle<()>>,
    worker_pool: ThreadLocalActorSpawner,
}

#[derive(Default)]
pub struct IrohEndpoint;

impl ThreadLocalActor for IrohEndpoint {
    type State = IrohState;

    type Msg = ToIrohEndpoint;

    type Arguments = (NetworkId, PrivateKey, IrohConfig, AddressBook);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (network_id, private_key, config, address_book) = args;

        // Automatically bind iroh endpoint after actor start.
        myself.send_message(ToIrohEndpoint::Bind)?;

        Ok(IrohState {
            network_id,
            private_key,
            config,
            address_book,
            endpoint: None,
            protocols: Arc::default(),
            accept_handle: None,
            watch_addr_handle: None,
            worker_pool: ThreadLocalActorSpawner::new(),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(endpoint) = state.endpoint.take() {
            // Make sure the endpoint has all the time it needs to gracefully shut down while other
            // processes might already drop the whole actor.
            tokio::task::spawn(async move {
                endpoint.close().await;
            });
        }

        if let Some(watch_addr_handle) = &state.watch_addr_handle {
            watch_addr_handle.abort();
        }

        if let Some(accept_handle) = &state.accept_handle {
            accept_handle.abort();
        }

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToIrohEndpoint::Bind => {
                let config = state.config.clone();

                // Configure sockets to bind to.
                let socket_address_v4 = SocketAddrV4::new(config.bind_ip_v4, config.bind_port_v4);
                let socket_address_v6 =
                    SocketAddrV6::new(config.bind_ip_v6, config.bind_port_v6, 0, 0);

                // Default QUIC transport parameters, can be overwritten when connecting to a node.
                let mut transport_config = TransportConfig::default();
                transport_config.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
                transport_config.max_idle_timeout(Some(
                    MAX_IDLE_TIMEOUT.try_into().expect("correct max idle value"),
                ));

                // Register list of possible "home relays" for this node.
                let relay_mode = {
                    let relay_map = iroh::RelayMap::from_iter(config.relay_urls);
                    iroh::RelayMode::Custom(relay_map)
                };

                // Connect iroh's endpoint with our own address book to "publish" our changed iroh
                // address directly and "resolve" endpoint id's.
                let address_book_discovery = AddressBookDiscovery::new(
                    state.private_key.clone(),
                    state.address_book.clone(),
                );

                // Create and bind the endpoint to the socket.
                let endpoint = iroh::Endpoint::empty_builder(relay_mode)
                    .discovery(address_book_discovery)
                    .secret_key(from_private_key(state.private_key.clone()))
                    .transport_config(transport_config)
                    .bind_addr_v4(socket_address_v4)
                    .bind_addr_v6(socket_address_v6)
                    .bind()
                    .await?;

                // Handle incoming connection requests from other nodes.
                let accept_handle = {
                    let endpoint = endpoint.clone();
                    tokio::spawn(async move {
                        loop {
                            let Some(incoming) = endpoint.accept().await else {
                                break; // Endpoint is closed.
                            };
                            let _ = myself.send_message(ToIrohEndpoint::Incoming(incoming));
                        }
                    })
                };

                state.endpoint = Some(endpoint);
                state.accept_handle = Some(accept_handle);
            }
            ToIrohEndpoint::RegisterProtocol(alpn, protocol_handler) => {
                let mixed_protocol_id = hash_protocol_id_with_network_id(&alpn, state.network_id);
                debug!(alpn = %mixed_protocol_id.fmt_short(), "register protocol");

                // Register protocol in our own map to accept it in the future.
                let mut protocols = state.protocols.write().await;
                protocols.insert(mixed_protocol_id, protocol_handler);

                // Inform iroh endpoint about the new protocol as well.
                state
                    .endpoint
                    .as_ref()
                    .expect(
                        "bind always takes place first, an endpoint must exist after this point",
                    )
                    .set_alpns(protocols.keys().cloned().collect());
            }
            ToIrohEndpoint::Connect(node_id, alpn, transport_config, reply) => {
                let mixed_protocol_id = hash_protocol_id_with_network_id(&alpn, state.network_id);

                // Ask address book for available node information.
                let result = match state.address_book.node_info(node_id).await {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = reply.send(Err(err.into()));
                        return Ok(());
                    }
                };

                let Some(node_info) = result else {
                    let _ = reply.send(Err(ConnectError::TransportInfoMissing(node_id)));
                    return Ok(());
                };

                // Check if node info contains address information for iroh transport.
                let Ok(endpoint_addr) = iroh::EndpointAddr::try_from(node_info) else {
                    let _ = reply.send(Err(ConnectError::TransportInfoMissing(node_id)));
                    return Ok(());
                };

                // This actor will shut down immediately after the connection was established. The
                // responsibility to handle the connection object is shifted to the caller from
                // this point on, so using this actor to reason about the state of the connection
                // is not possible here.
                IrohConnection::spawn_linked(
                    None,
                    (
                        state.private_key.public_key(),
                        IrohConnectionArgs::Connect {
                            endpoint: state.endpoint
                                .clone()
                                .expect(
                                    "bind always takes place first, an endpoint must exist after this point"
                                ),
                            endpoint_addr: endpoint_addr.clone(),
                            alpn: mixed_protocol_id,
                            transport_config,
                            reply,
                        },
                        myself.clone(),
                    ),
                    myself.into(),
                    state.worker_pool.clone(),
                )
                .await?;
            }
            ToIrohEndpoint::Incoming(incoming) => {
                // This actor runs as long as the protocol session holds the "accept" method. If
                // the implementation decides to move the `Connection` object out of it, this actor
                // will terminate, but the connection will persist.
                //
                // This means: The lifetime of this actor does _not_ indicate the lifetime of the
                // connection itself.
                IrohConnection::spawn_linked(
                    None,
                    (
                        state.private_key.public_key(),
                        IrohConnectionArgs::Accept {
                            incoming,
                            protocols: state.protocols.clone(),
                        },
                        myself.clone(),
                    ),
                    myself.into(),
                    state.worker_pool.clone(),
                )
                .await?;
            }
            ToIrohEndpoint::Endpoint(reply) => {
                let _ = reply.send(state.endpoint.clone().expect(
                    "bind always takes place first, an endpoint must exist after this point",
                ));
            }
            ToIrohEndpoint::Report {
                remote_node_id,
                outcome,
                role,
                ..
            } => {
                // Sometimes we try to connect to another node while our own node has limited
                // connectivity, for example if we don't have a connection to the internet and only
                // to other devices on the local network, or even worse, only to other processes
                // via the loopback interface.
                //
                // In these cases we _don't_ want to report failed connections, as we could have
                // never reached them.
                //
                // TODO: We could check if both endpoints are only locally reachable, then this
                // failure report would be legit. For now we're considering this an edge case and
                // also ignore it.
                if let ConnectionRole::Connect { .. } = role
                    && outcome.is_failed()
                    && !is_globally_reachable_endpoint(state.endpoint.as_ref().expect(
                            "bind always takes place first, an endpoint must exist after this point").addr()) {
                        return Ok(());
                    }

                if state
                    .address_book
                    .report(remote_node_id, outcome)
                    .await
                    .is_err()
                {
                    warn!("could not record connection outcome in address book");
                }
            }
        }

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // NOTE: We're not supervising any child actors here right now but override the default
        // impl of `handle_supervisor_evt` anyhow, to prevent potential footguns in the future: Any
        // termination of any child actor would cause the endpoint to go down as well otherwise.
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error(transparent)]
    AddressBook(#[from] AddressBookError),

    #[error("address book does not have any iroh address info for node id {0}")]
    TransportInfoMissing(NodeId),

    #[error(transparent)]
    Connection(#[from] ConnectionActorError),
}
