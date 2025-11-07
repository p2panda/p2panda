// SPDX-License-Identifier: MIT OR Apache-2.0

//! Actor managing an endpoint to establish direct or relayed connections over the Internet
//! Protocol using the "iroh" crate.
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use std::time::Duration;

use iroh::protocol::DynProtocolHandler;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, registry};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::warn;

use crate::actors::events::ToEvents;
use crate::actors::iroh::connection::{ConnectionReplyPort, IrohConnection, IrohConnectionArgs};
use crate::args::ApplicationArguments;
use crate::from_private_key;
use crate::protocols::{ProtocolId, hash_protocol_id_with_network_id};

pub const IROH_ENDPOINT: &str = "net.iroh.endpoint";

/// Maximum number of streams accepted on a QUIC connection.
const DEFAULT_MAX_STREAMS: u32 = 1024;

#[allow(clippy::large_enum_variant)]
pub enum ToIrohEndpoint {
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
}

pub type ProtocolMap = Arc<RwLock<BTreeMap<ProtocolId, Box<dyn DynProtocolHandler>>>>;

pub struct IrohState {
    args: ApplicationArguments,
    endpoint: iroh::Endpoint,
    protocols: ProtocolMap,
    accept_handle: JoinHandle<()>,
    worker_pool: ThreadLocalActorSpawner,
}

pub struct IrohEndpoint;

impl Actor for IrohEndpoint {
    type State = IrohState;

    type Msg = ToIrohEndpoint;

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let config = args.iroh_config.clone();

        // Configure QUIC transport and sockets to bind to.
        let mut transport_config = iroh::endpoint::TransportConfig::default();
        transport_config
            .max_concurrent_bidi_streams(DEFAULT_MAX_STREAMS.into())
            .max_concurrent_uni_streams(0u32.into());

        let socket_address_v4 = SocketAddrV4::new(config.bind_ip_v4, config.bind_port_v4);
        let socket_address_v6 = SocketAddrV6::new(config.bind_ip_v6, config.bind_port_v6, 0, 0);

        // Register list of possible "home relays" for this node.
        let relay_provided = !config.relay_urls.is_empty();
        let relay_map = iroh::RelayMap::from_iter(config.relay_urls);
        let relay_mode = iroh::RelayMode::Custom(relay_map);

        // Create and bind the endpoint to the socket.
        // @TODO: Add static provider to register addresses coming from our discovery mechanism.
        let endpoint = iroh::Endpoint::builder()
            .secret_key(from_private_key(args.private_key.clone()))
            .transport_config(transport_config)
            .relay_mode(relay_mode)
            .bind_addr_v4(socket_address_v4)
            .bind_addr_v6(socket_address_v6)
            .bind()
            .await?;

        let accept_handle = {
            let endpoint = endpoint.clone();
            tokio::spawn(async move {
                loop {
                    let Some(incoming) = endpoint.accept().await else {
                        break; // Endpoint is closed.
                    };
                    myself.send_message(ToIrohEndpoint::Incoming(incoming));
                }
            })
        };

        Ok(IrohState {
            args,
            endpoint,
            protocols: Arc::default(),
            accept_handle,
            worker_pool: ThreadLocalActorSpawner::new(),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        state.endpoint.close().await;
        state.accept_handle.abort();
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToIrohEndpoint::RegisterProtocol(alpn, protocol_handler) => {
                let mixed_protocol_id =
                    hash_protocol_id_with_network_id(&alpn, &state.args.network_id);
                let mut protocols = state.protocols.write().await;
                protocols.insert(mixed_protocol_id.to_vec(), protocol_handler);
            }
            ToIrohEndpoint::Connect(node_addr, alpn, reply) => {
                let mixed_protocol_id =
                    hash_protocol_id_with_network_id(&alpn, &state.args.network_id);

                // This actor will shut down immediately after the connection was established. The
                // responsibility to handle the connection object is shifted to the caller from
                // this point on, so using this actor to reason about the state of the connection
                // is not possible here.
                IrohConnection::spawn(
                    None,
                    IrohConnectionArgs::Connect {
                        endpoint: state.endpoint.clone(),
                        node_addr,
                        alpn: mixed_protocol_id.to_vec(),
                        reply,
                    },
                    state.worker_pool.clone(),
                )
                .await?;
            }
            ToIrohEndpoint::Incoming(incoming) => {
                // This actor will run as long as the protocol session.
                IrohConnection::spawn(
                    None,
                    IrohConnectionArgs::Accept {
                        incoming,
                        protocols: state.protocols.clone(),
                    },
                    state.worker_pool.clone(),
                )
                .await?;
            }
            ToIrohEndpoint::Endpoint(reply) => {
                let _ = reply.send(state.endpoint.clone());
            }
        }

        Ok(())
    }
}
