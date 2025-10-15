// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint actor.
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
use iroh::endpoint::TransportConfig as IrohTransportConfig;
use iroh::protocol::Router as IrohRouter;
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent, registry};
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::actors::events::ToEvents;
use crate::actors::subscription::{Subscription, ToSubscription};
use crate::addrs::RelayUrl;
use crate::defaults::{DEFAULT_BIND_PORT, DEFAULT_MAX_STREAMS};
use crate::from_private_key;
use crate::protocols::ProtocolMap;

#[derive(Debug)]
// TODO: Remove once used.
#[allow(dead_code)]
pub(crate) struct EndpointConfig {
    pub(crate) bind_ip_v4: Ipv4Addr,
    pub(crate) bind_port_v4: u16,
    pub(crate) bind_ip_v6: Ipv6Addr,
    pub(crate) bind_port_v6: u16,
    pub(crate) protocols: ProtocolMap,
    pub(crate) relays: Vec<RelayUrl>,
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

pub(crate) enum ToEndpoint {}

impl Message for ToEndpoint {}

pub(crate) struct EndpointState {
    endpoint: IrohEndpoint,
    router: IrohRouter,
    subscription_actor: ActorRef<ToSubscription>,
    subscription_actor_failures: u16,
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

        let mut transport_config = IrohTransportConfig::default();
        transport_config
            .max_concurrent_bidi_streams(DEFAULT_MAX_STREAMS.into())
            .max_concurrent_uni_streams(0u32.into());

        let relays: Vec<IrohRelayUrl> = config.relays.into_iter().map(|url| url.into()).collect();
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
            if let Some(events_actor) = registry::where_is("events".to_string()) {
                events_actor.send_message(ToEvents::ConnectedToRelay)?
            }
        } else {
            warn!("endpoint actor: failed to connect to relay")
        }

        let mut router_builder = IrohRouter::builder(endpoint.clone());

        // Register protocols with router.
        let mut protocols = config.protocols;
        while let Some((identifier, handler)) = protocols.pop_first() {
            router_builder = router_builder.accept(identifier, handler);
        }

        let router = router_builder.spawn();

        // Spawn the subscription actor.
        let (subscription_actor, _) = Actor::spawn_linked(
            Some("subscription".to_string()),
            Subscription,
            endpoint.clone(),
            myself.clone().into(),
        )
        .await?;

        let state = EndpointState {
            endpoint,
            router,
            subscription_actor,
            subscription_actor_failures: 0,
        };

        Ok(state)
    }

    async fn post_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Shutdown all protocol handlers and close the iroh `Endpoint`.
        state.router.shutdown().await?;

        state
            .subscription_actor
            .stop(Some("endpoint actor is shutting down".to_string()));

        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor) => {
                if let Some(name) = actor.get_name() {
                    debug!("endpoint actor: received ready from {} actor", name);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some("subscription") = actor.get_name().as_deref() {
                    warn!("endpoint actor: subscription actor failed: {}", panic_msg);

                    // Respawn the subscription actor.
                    let (subscription_actor, _) = Actor::spawn_linked(
                        Some("subscription".to_string()),
                        Subscription,
                        state.endpoint.clone(),
                        myself.clone().into(),
                    )
                    .await?;

                    state.subscription_actor_failures += 1;
                    state.subscription_actor = subscription_actor;
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!("endpoint actor: {} actor terminated", name);
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;
    use ractor::Actor;
    use serial_test::serial;
    use tokio::time::{Duration, sleep};
    use tracing_test::traced_test;

    use super::{Endpoint, EndpointConfig};

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn endpoint_child_actors_are_started() {
        let private_key = PrivateKey::new();

        let endpoint_config = EndpointConfig::default();
        let (endpoint_actor, endpoint_actor_handle) = Actor::spawn(
            Some("endpoint".to_string()),
            Endpoint,
            (private_key, endpoint_config),
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        endpoint_actor.stop(None);
        endpoint_actor_handle.await.unwrap();

        assert!(logs_contain(
            "endpoint actor: received ready from subscription actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
