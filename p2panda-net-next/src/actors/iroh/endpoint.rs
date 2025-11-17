// SPDX-License-Identifier: MIT OR Apache-2.0

//! Actor managing an endpoint to establish direct or relayed connections over the Internet
//! Protocol using the "iroh" crate.
use std::collections::BTreeMap;
use std::net::{SocketAddrV4, SocketAddrV6};
use std::sync::Arc;

use futures_util::StreamExt;
use iroh::Watcher;
use iroh::protocol::DynProtocolHandler;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, registry};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::iroh::connection::{ConnectionReplyPort, IrohConnection, IrohConnectionArgs};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::args::ApplicationArguments;
use crate::protocols::{ProtocolId, hash_protocol_id_with_network_id};
use crate::utils::{ShortFormat, from_private_key};
use crate::{NodeInfo, TopicId, UnsignedTransportInfo};

pub const IROH_ENDPOINT: &str = "net.iroh.endpoint";

/// Maximum number of streams accepted on a QUIC connection.
const DEFAULT_MAX_STREAMS: u32 = 1024;

#[allow(clippy::large_enum_variant)]
pub enum ToIrohEndpoint {
    /// Bind the iroh endpoint.
    ///
    /// This takes place automatically after the actor started.
    Bind,

    /// Return the internal iroh endpoint instance (used by iroh-gossip).
    Endpoint(RpcReplyPort<iroh::endpoint::Endpoint>),

    /// Register protocol handler for a given ALPN (protocol identifier).
    RegisterProtocol(ProtocolId, Box<dyn DynProtocolHandler>),

    /// Starts a connection attempt to a remote iroh endpoint and returns a future which can be
    /// awaited for establishing the final connection.
    ///
    /// The `NodeAddr` must contain the `NodeId` to dial and may also contain a `RelayUrl` and
    /// direct addresses. If direct addresses are provided, they will be used to try and establish
    /// a direct connection without involving a relay server.
    ///
    /// The ALPN byte string, or application-level protocol identifier, is also required. The
    /// remote endpoint must support this alpn, otherwise the connection attempt will fail with an
    /// error.
    Connect(iroh::EndpointAddr, ProtocolId, ConnectionReplyPort),

    /// We've received a connection attempt from a remote iroh endpoint.
    Incoming(iroh::endpoint::Incoming),

    /// Our own endpoint address has changed.
    AddressChanged(Option<iroh::EndpointAddr>),
}

pub type ProtocolMap = Arc<RwLock<BTreeMap<ProtocolId, Box<dyn DynProtocolHandler>>>>;

pub struct IrohState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
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

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // Automatically bind iroh endpoint after actor start.
        myself.send_message(ToIrohEndpoint::Bind)?;

        Ok(IrohState {
            actor_namespace,
            args,
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
        if let Some(endpoint) = &state.endpoint {
            endpoint.close().await;
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
                let config = state.args.iroh_config.clone();

                // Configure QUIC transport and sockets to bind to.
                let mut transport_config = iroh::endpoint::TransportConfig::default();
                transport_config
                    .max_concurrent_bidi_streams(DEFAULT_MAX_STREAMS.into())
                    .max_concurrent_uni_streams(0u32.into());

                let socket_address_v4 = SocketAddrV4::new(config.bind_ip_v4, config.bind_port_v4);
                let socket_address_v6 =
                    SocketAddrV6::new(config.bind_ip_v6, config.bind_port_v6, 0, 0);

                // Register list of possible "home relays" for this node.
                let relay_map = iroh::RelayMap::from_iter(config.relay_urls);
                let relay_mode = iroh::RelayMode::Custom(relay_map);

                // Create and bind the endpoint to the socket.
                // @TODO: Add static provider to register addresses coming from our discovery
                // mechanism.
                let endpoint = iroh::Endpoint::builder()
                    .secret_key(from_private_key(state.args.private_key.clone()))
                    .transport_config(transport_config)
                    .relay_mode(relay_mode)
                    .bind_addr_v4(socket_address_v4)
                    .bind_addr_v6(socket_address_v6)
                    .bind()
                    .await?;

                // Watch for changes of our own endpoint address.
                let watch_addr_handle = {
                    let mut watcher = endpoint.watch_addr().stream();
                    let myself = myself.clone();
                    tokio::spawn(async move {
                        loop {
                            let addr = watcher.next().await;
                            let _ = myself.send_message(ToIrohEndpoint::AddressChanged(addr));
                        }
                    })
                };

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
                state.watch_addr_handle = Some(watch_addr_handle);
            }
            ToIrohEndpoint::RegisterProtocol(alpn, protocol_handler) => {
                let mixed_protocol_id =
                    hash_protocol_id_with_network_id(&alpn, &state.args.network_id);
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
            ToIrohEndpoint::Connect(endpoint_addr, alpn, reply) => {
                let mixed_protocol_id =
                    hash_protocol_id_with_network_id(&alpn, &state.args.network_id);

                // This actor will shut down immediately after the connection was established. The
                // responsibility to handle the connection object is shifted to the caller from
                // this point on, so using this actor to reason about the state of the connection
                // is not possible here.
                IrohConnection::spawn(
                    None,
                    (
                        state.args.public_key,
                        IrohConnectionArgs::Connect {
                            endpoint: state.endpoint
                                .clone()
                                .expect(
                                    "bind always takes place first, an endpoint must exist after this point"
                                ),
                            endpoint_addr,
                            alpn: mixed_protocol_id,
                            reply,
                        },
                    ),
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
                IrohConnection::spawn(
                    None,
                    (
                        state.args.public_key,
                        IrohConnectionArgs::Accept {
                            incoming,
                            protocols: state.protocols.clone(),
                        },
                    ),
                    state.worker_pool.clone(),
                )
                .await?;
            }
            ToIrohEndpoint::Endpoint(reply) => {
                let _ = reply.send(state.endpoint.clone().expect(
                    "bind always takes place first, an endpoint must exist after this point",
                ));
            }
            ToIrohEndpoint::AddressChanged(addr) => {
                debug!(?addr, "updated iroh endpoint address");

                // Create a new transport info with iroh addresses if given. If no iroh address
                // exists (because we are not reachable) we're explicitly making the address array
                // empty to inform other nodes about this.
                let transport_info = match addr {
                    Some(addr) => UnsignedTransportInfo::from_addrs([addr.into()]),
                    None => UnsignedTransportInfo::new(),
                }
                .sign(&state.args.private_key)?;

                let Some(actor) =
                    registry::where_is(with_namespace(ADDRESS_BOOK, &state.actor_namespace))
                else {
                    // Address book is not reachable, so we're probably shutting down.
                    return Ok(());
                };
                // @TODO: T is TopicId here. This needs to be refactored as part of the general
                // topic changeover.
                let address_book_ref = ActorRef::<ToAddressBook<TopicId>>::from(actor);

                // Update existing node info about us if available or create a new one.
                let mut node_info = match call!(
                    address_book_ref,
                    ToAddressBook::NodeInfo,
                    state.args.public_key
                )? {
                    Some(node_info) => node_info,
                    None => NodeInfo::new(state.args.public_key),
                };
                node_info.update_transports(transport_info)?;
                let _ = call!(address_book_ref, ToAddressBook::InsertNodeInfo, node_info)?;
            }
        }

        Ok(())
    }
}
