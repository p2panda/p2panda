// SPDX-License-Identifier: MIT OR Apache-2.0

//! Network actor.
//!
//! The root of the entire system supervision tree; it's only role is to spawn and
//! supervise other actors.
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::address_book::{AddressBook, ToAddressBook};
use crate::actors::discovery::{Discovery, ToDiscovery};
use crate::actors::endpoint::{Endpoint, EndpointConfig, ToEndpoint};
use crate::actors::events::{Events, ToEvents};

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NetworkConfig {
    pub(crate) endpoint_config: EndpointConfig,
}

pub enum ToNetwork {}

impl Message for ToNetwork {}

pub struct NetworkState {
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    endpoint_actor: ActorRef<ToEndpoint>,
    address_book_actor: ActorRef<ToAddressBook>,
    address_book_actor_failures: u16,
    discovery_actor: ActorRef<ToDiscovery>,
    discovery_actor_failures: u16,
}

pub struct Network;

impl Actor for Network {
    type State = NetworkState;
    type Msg = ToNetwork;
    type Arguments = (PrivateKey, NetworkConfig);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (private_key, config) = args;

        // Spawn the events actor.
        let (events_actor, _) = Actor::spawn_linked(
            Some("events".to_string()),
            Events,
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the endpoint actor.
        let (endpoint_actor, _) = Actor::spawn_linked(
            Some("endpoint".to_string()),
            Endpoint,
            (private_key, config.endpoint_config),
            myself.clone().into(),
        )
        .await?;

        // Spawn the address book actor.
        let (address_book_actor, _) = Actor::spawn_linked(
            Some("address book".to_string()),
            AddressBook {},
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the discovery actor.
        let (discovery_actor, _) = Actor::spawn_linked(
            Some("discovery".to_string()),
            Discovery {},
            (),
            myself.clone().into(),
        )
        .await?;

        let state = NetworkState {
            events_actor,
            events_actor_failures: 0,
            endpoint_actor,
            address_book_actor,
            address_book_actor_failures: 0,
            discovery_actor,
            discovery_actor_failures: 0,
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
        let reason = Some("network system is shutting down".to_string());

        // Stop all the actors which are supervised by the network actor.
        state.events_actor.stop(reason.clone());
        state.endpoint_actor.stop(reason.clone());
        state.address_book_actor.stop(reason.clone());
        state.discovery_actor.stop(reason);

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
                    debug!("network actor: received ready from {} actor", name);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                match actor.get_name().as_deref() {
                    Some("events") => {
                        warn!("network actor: events actor failed: {}", panic_msg);

                        // Respawn the events actor.
                        let (events_actor, _) = Actor::spawn_linked(
                            Some("events".to_string()),
                            Events {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.events_actor_failures += 1;
                        state.events_actor = events_actor;
                    }
                    Some("endpoint") => {
                        warn!("network actor: endpoint actor failed: {}", panic_msg);

                        // If the endpoint actor fails then the entire system is compromised and we
                        // stop the top-level network actor.
                        myself.stop(Some("endpoint actor failed".to_string()));
                    }
                    Some("address book") => {
                        warn!("network actor: address book actor failed: {}", panic_msg);

                        // Respawn the address book actor.
                        let (address_book_actor, _) = Actor::spawn_linked(
                            Some("address book".to_string()),
                            AddressBook {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.address_book_actor_failures += 1;
                        state.address_book_actor = address_book_actor;
                    }
                    Some("discovery") => {
                        warn!("network actor: discovery actor failed: {}", panic_msg);

                        // Respawn the discovery actor.
                        let (discovery_actor, _) = Actor::spawn_linked(
                            Some("discovery".to_string()),
                            Discovery {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.discovery_actor_failures += 1;
                        state.discovery_actor = discovery_actor;
                    }
                    _ => (),
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!("network actor: {} actor terminated", name);
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ractor::Actor;
    use serial_test::serial;
    use tokio::time::{sleep, Duration};
    use tracing_test::traced_test;

    use super::Network;

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn network_child_actors_are_started() {
        let network_config = Default::default();

        let (network_actor, network_actor_handle) =
            Actor::spawn(Some("network".to_string()), Network, network_config)
                .await
                .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        network_actor.stop(None);
        network_actor_handle.await.unwrap();

        assert!(logs_contain(
            "network actor: received ready from events actor"
        ));
        assert!(logs_contain(
            "network actor: received ready from endpoint actor"
        ));
        assert!(logs_contain(
            "network actor: received ready from address book actor"
        ));
        assert!(logs_contain(
            "network actor: received ready from discovery actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
