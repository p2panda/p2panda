// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint actor.
//!
//! This actor is responsible for creating an iroh `Endpoint` with an associated `Router` and
//! registering network protocols with the `Router`.
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;

use iroh::Endpoint as IrohEndpoint;
use iroh::RelayMap as IrohRelayMap;
use iroh::RelayMode as IrohRelayMode;
use iroh::RelayUrl as IrohRelayUrl;
use iroh::endpoint::TransportConfig as IrohTransportConfig;
use iroh::protocol::Router as IrohRouter;
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, registry};
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::actors::events::{EVENTS, ToEvents};
use crate::actors::stream::{STREAM, Stream, ToStream};
use crate::actors::{generate_actor_namespace, with_namespace, without_namespace};
use crate::defaults::{DEFAULT_BIND_PORT, DEFAULT_MAX_STREAMS};
use crate::protocols::ProtocolMap;
use crate::{from_private_key, to_public_key};

/// Endpoint actor name.
pub const ENDPOINT: &str = "net.endpoint";

/// Configures the endpoint actor which uses an iroh `Endpoint` internally.
#[derive(Debug)]
// TODO: Remove once used.
#[allow(dead_code)]
pub struct EndpointConfig {
    pub(crate) bind_ip_v4: Ipv4Addr,
    pub(crate) bind_port_v4: u16,
    pub(crate) bind_ip_v6: Ipv6Addr,
    pub(crate) bind_port_v6: u16,
    pub(crate) protocols: ProtocolMap,
    pub(crate) relays: Vec<IrohRelayUrl>,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            bind_ip_v4: Ipv4Addr::UNSPECIFIED,
            bind_port_v4: DEFAULT_BIND_PORT,
            bind_ip_v6: Ipv6Addr::UNSPECIFIED,
            bind_port_v6: DEFAULT_BIND_PORT + 1,
            protocols: Default::default(),
            relays: Vec::new(),
        }
    }
}

pub(crate) enum ToEndpoint {
    /// Return a clone of the iroh endpoint.
    Endpoint(RpcReplyPort<IrohEndpoint>),
}

pub(crate) struct EndpointState {
    endpoint: IrohEndpoint,
    router: IrohRouter,
}

pub(crate) struct Endpoint;

impl Actor for Endpoint {
    type State = EndpointState;
    type Msg = ToEndpoint;
    type Arguments = (PrivateKey, EndpointConfig);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (private_key, config) = args;

        let actor_namespace = generate_actor_namespace(&private_key.public_key());

        let mut transport_config = IrohTransportConfig::default();
        transport_config
            .max_concurrent_bidi_streams(DEFAULT_MAX_STREAMS.into())
            .max_concurrent_uni_streams(0u32.into());

        let relays: Vec<IrohRelayUrl> = config.relays;
        let relay_provided = !relays.is_empty();
        let relay_map = IrohRelayMap::from_iter(relays);
        let relay_mode = IrohRelayMode::Custom(relay_map);

        let socket_address_v4 = SocketAddrV4::new(config.bind_ip_v4, config.bind_port_v4);
        let socket_address_v6 = SocketAddrV6::new(config.bind_ip_v6, config.bind_port_v6, 0, 0);

        let endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(private_key))
            .transport_config(transport_config)
            .relay_mode(relay_mode)
            .bind_addr_v4(socket_address_v4)
            .bind_addr_v6(socket_address_v6)
            .bind()
            .await?;

        // Wait for the endpoint to initiate a connection with a relay.
        if relay_provided
            && timeout(Duration::from_secs(5), endpoint.online())
                .await
                .is_ok()
        {
            // Inform the events actor of the connection.
            if let Some(events_actor) = registry::where_is(with_namespace(EVENTS, &actor_namespace))
            {
                events_actor.send_message(ToEvents::ConnectedToRelay)?
            }
        } else {
            warn!("{ENDPOINT} actor: failed to connect to relay")
        }

        let mut router_builder = IrohRouter::builder(endpoint.clone());

        // Register protocols with router.
        let mut protocols = config.protocols;
        while let Some((identifier, handler)) = protocols.pop_first() {
            router_builder = router_builder.accept(identifier, handler);
        }

        let router = router_builder.spawn();

        let state = EndpointState { endpoint, router };

        Ok(state)
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Shutdown all protocol handlers and close the iroh `Endpoint`.
        state.router.shutdown().await?;

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToEndpoint::Endpoint(reply) => {
                let endpoint = state.endpoint.clone();
                let _ = reply.send(endpoint);
            }
        }

        Ok(())
    }
}
