// SPDX-License-Identifier: MIT OR Apache-2.0

//! Supervision actor.
//!
//! The root of the entire system supervision tree; it's only role is to spawn and
//! supervise other actors.
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::address_book::{AddressBook, ToAddressBook};
use crate::actors::discovery::{Discovery, ToDiscovery};
use crate::actors::endpoint::{Endpoint, EndpointConfig, ToEndpoint};
use crate::actors::events::{Events, ToEvents};
use crate::utils::{with_suffix, without_suffix};

// TODO: Rename or move...feels out of place here now.
// adz has an `Arguments` struct in his code; use that.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NetworkConfig {
    pub(crate) endpoint_config: EndpointConfig,
}

pub struct SupervisorState {
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    endpoint_actor: ActorRef<ToEndpoint>,
    address_book_actor: ActorRef<ToAddressBook>,
    address_book_actor_failures: u16,
    discovery_actor: ActorRef<ToDiscovery>,
    discovery_actor_failures: u16,
    public_key_suffix: String,
}

pub struct Supervisor;

impl Actor for Supervisor {
    type State = SupervisorState;
    type Msg = ();
    type Arguments = (PrivateKey, NetworkConfig);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (private_key, config) = args;

        let public_key_suffix = private_key.public_key().to_hex()[..6].to_string();

        // Spawn the events actor.
        let (events_actor, _) = Actor::spawn_linked(
            Some(with_suffix("events", &public_key_suffix)),
            Events,
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the endpoint actor.
        let (endpoint_actor, _) = Actor::spawn_linked(
            Some(with_suffix("endpoint", &public_key_suffix)),
            Endpoint,
            (private_key, config.endpoint_config),
            myself.clone().into(),
        )
        .await?;

        // Spawn the address book actor.
        let (address_book_actor, _) = Actor::spawn_linked(
            Some(with_suffix("address book", &public_key_suffix)),
            AddressBook {},
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the discovery actor.
        let (discovery_actor, _) = Actor::spawn_linked(
            Some(with_suffix("discovery", &public_key_suffix)),
            Discovery {},
            (),
            myself.clone().into(),
        )
        .await?;

        let state = SupervisorState {
            events_actor,
            events_actor_failures: 0,
            endpoint_actor,
            address_book_actor,
            address_book_actor_failures: 0,
            discovery_actor,
            discovery_actor_failures: 0,
            public_key_suffix,
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

        // Stop all the actors which are directly upervised by this actor.
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
                    debug!(
                        "supervisor actor: received ready from {} actor",
                        without_suffix(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_suffix("events", &state.public_key_suffix) {
                        warn!("supervisor actor: events actor failed: {}", panic_msg);

                        // Respawn the events actor.
                        let (events_actor, _) = Actor::spawn_linked(
                            Some(with_suffix("events", &state.public_key_suffix)),
                            Events {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.events_actor_failures += 1;
                        state.events_actor = events_actor;
                    } else if name == with_suffix("endpoint", &state.public_key_suffix) {
                        warn!("supervisor actor: endpoint actor failed: {}", panic_msg);

                        // If the endpoint actor fails then the entire system is compromised and we
                        // stop the top-level supervisor actor.
                        myself.stop(Some("endpoint actor failed".to_string()));
                    } else if name == with_suffix("address book", &state.public_key_suffix) {
                        warn!("supervisor actor: address book actor failed: {}", panic_msg);

                        // Respawn the address book actor.
                        let (address_book_actor, _) = Actor::spawn_linked(
                            Some(with_suffix("address book", &state.public_key_suffix)),
                            AddressBook {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.address_book_actor_failures += 1;
                        state.address_book_actor = address_book_actor;
                    } else if name == with_suffix("discovery", &state.public_key_suffix) {
                        warn!("supervisor actor: discovery actor failed: {}", panic_msg);

                        // Respawn the discovery actor.
                        let (discovery_actor, _) = Actor::spawn_linked(
                            Some(with_suffix("discovery", &state.public_key_suffix)),
                            Discovery {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.discovery_actor_failures += 1;
                        state.discovery_actor = discovery_actor;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "supervisor actor: {} actor terminated",
                        without_suffix(&name)
                    );
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

    use crate::utils::with_suffix;

    use super::Supervisor;

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn supervisor_child_actors_are_started() {
        let private_key: PrivateKey = Default::default();
        let public_key_suffix = &private_key.public_key().to_hex()[..6];

        let network_config = Default::default();

        let (supervisor_actor, supervisor_actor_handle) = Actor::spawn(
            Some(with_suffix("supervisor", &public_key_suffix)),
            Supervisor,
            (private_key, network_config),
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        supervisor_actor.stop(None);
        supervisor_actor_handle.await.unwrap();

        assert!(logs_contain(
            "supervisor actor: received ready from events actor"
        ));
        assert!(logs_contain(
            "supervisor actor: received ready from endpoint actor"
        ));
        assert!(logs_contain(
            "supervisor actor: received ready from address book actor"
        ));
        assert!(logs_contain(
            "supervisor actor: received ready from discovery actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
