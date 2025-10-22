// SPDX-License-Identifier: MIT OR Apache-2.0

//! Actor managing an endpoint to establish direct or relayed connections over the Internet
//! Protocol using the "iroh" crate.
//!
//! This actor is responsible for creating an iroh `Endpoint` with an associated `Router`,
//! registering network protocols with the `Router` and spawning the subscription actor. It also
//! performs supervision of the spawned actor, restarting it in the event of failure.
//!
//! The subscription actor is a child of the endpoint actor. This design decision was made because
//! it currently relies on an iroh `Endpoint` (for gossip and sync connections). If something goes
//! wrong with the gossip or sync actors, they can be respawned by the endpoint actor. If the
//! endpoint actor itself fails, the entire network system is shutdown.
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;

use iroh::Endpoint as IrohEndpoint;
use iroh::RelayMap as IrohRelayMap;
use iroh::RelayMode as IrohRelayMode;
use iroh::RelayUrl as IrohRelayUrl;
use iroh::endpoint::ConnectWithOptsError as IrohConnectWithOptsError;
use iroh::endpoint::Connecting as IrohConnecting;
use iroh::endpoint::TransportConfig as IrohTransportConfig;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, registry};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::warn;

use crate::actors::endpoint::router::{IROH_ROUTER, ToIrohRouter};
use crate::actors::events::ToEvents;
use crate::args::ApplicationArguments;
use crate::defaults::{DEFAULT_BIND_PORT, DEFAULT_MAX_STREAMS};
use crate::protocols::ProtocolId;
use crate::utils::from_private_key;

pub const IROH_TRANSPORT: &str = "net.endpoint.transports.iroh";

#[derive(Clone, Debug)]
pub struct IrohConfig {
    pub bind_ip_v4: Ipv4Addr,
    pub bind_port_v4: u16,
    pub bind_ip_v6: Ipv6Addr,
    pub bind_port_v6: u16,
    pub relay_urls: Vec<IrohRelayUrl>,
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            bind_ip_v4: Ipv4Addr::UNSPECIFIED,
            bind_port_v4: DEFAULT_BIND_PORT,
            bind_ip_v6: Ipv6Addr::UNSPECIFIED,
            bind_port_v6: DEFAULT_BIND_PORT + 1,
            relay_urls: Vec::new(),
        }
    }
}

pub enum ToIroh {
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
    Connect(
        iroh::NodeAddr,
        ProtocolId,
        RpcReplyPort<Result<IrohConnecting, IrohConnectWithOptsError>>,
    ),
}

pub struct IrohState {
    endpoint: IrohEndpoint,
    accept_handle: JoinHandle<()>,
}

pub struct IrohTransport;

impl Actor for IrohTransport {
    type State = IrohState;

    type Msg = ToIroh;

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let config = args.config.iroh;

        // Configure QUIC transport and sockets to bind to.
        let mut transport_config = IrohTransportConfig::default();
        transport_config
            .max_concurrent_bidi_streams(DEFAULT_MAX_STREAMS.into())
            .max_concurrent_uni_streams(0u32.into());

        let socket_address_v4 = SocketAddrV4::new(config.bind_ip_v4, config.bind_port_v4);
        let socket_address_v6 = SocketAddrV6::new(config.bind_ip_v6, config.bind_port_v6, 0, 0);

        // Register list of possible "home relays" for this node.
        let relay_provided = !config.relay_urls.is_empty();
        let relay_map = IrohRelayMap::from_iter(config.relay_urls);
        let relay_mode = IrohRelayMode::Custom(relay_map);

        // Create and bind the endpoint to the socket.
        // @TODO: Add static provider to register addresses coming from our discovery mechanism.
        let endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(args.private_key))
            .transport_config(transport_config)
            .relay_mode(relay_mode)
            .bind_addr_v4(socket_address_v4)
            .bind_addr_v6(socket_address_v6)
            .bind()
            .await?;

        // @TODO(adz): This runs only once and then never again if no connection was established
        // after 5 seconds. I assume we want something which consistently reports to us if we're
        // connected with a relay or not (and back).
        {
            let endpoint = endpoint.clone();

            tokio::spawn(async move {
                // Wait for the endpoint to initiate a connection with a relay.
                if relay_provided
                    && timeout(Duration::from_secs(5), endpoint.online())
                        .await
                        .is_ok()
                {
                    // Inform the events actor of the connection.
                    if let Some(events_actor) = registry::where_is("events".to_string()) {
                        events_actor
                            .send_message(ToEvents::ConnectedToRelay)
                            .unwrap();
                    }
                } else {
                    warn!("endpoint actor: failed to connect to relay")
                }
            });
        }

        let accept_handle = {
            let endpoint = endpoint.clone();

            tokio::spawn(async move {
                loop {
                    let Some(incoming) = endpoint.accept().await else {
                        break; // Endpoint is closed.
                    };

                    if let Some(router_actor) = registry::where_is(IROH_ROUTER.into()) {
                        let _ = router_actor.send_message(ToIrohRouter::Incoming(incoming));
                    }
                }
            })
        };

        Ok(IrohState {
            endpoint,
            accept_handle,
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
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToIroh::Connect(node_addr, alpn, reply) => {
                // Don't block the actor here by waiting for a connection to be established, we
                // return the "waiting to be connected" future `Connecting` instead and handle this
                // connection attempt somewhere else.
                let connecting = state
                    .endpoint
                    .connect_with_opts(node_addr, &alpn, Default::default())
                    .await;
                let _ = reply.send(connecting);
            }
        }

        Ok(())
    }
}
