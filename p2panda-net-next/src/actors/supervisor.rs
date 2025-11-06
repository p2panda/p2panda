// SPDX-License-Identifier: MIT OR Apache-2.0

//! Supervision actor.
//!
//! The root of the entire system supervision tree; it's only role is to spawn and
//! supervise other actors.
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{DISCOVERY, Discovery, ToDiscovery};
use crate::actors::endpoint::{ENDPOINT, Endpoint, EndpointConfig, ToEndpoint};
use crate::actors::events::{EVENTS, Events, ToEvents};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};

/// Supervisor actor name.
pub const SUPERVISOR: &str = "net.supervisor";

// TODO: Rename or move...feels out of place here now.
// adz has an `Arguments` struct in his code; use that.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct Config {
    pub(crate) endpoint: EndpointConfig,
}

pub struct SupervisorState {
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    endpoint_actor: ActorRef<ToEndpoint>,
    address_book_actor: ActorRef<ToAddressBook>,
    address_book_actor_failures: u16,
    discovery_actor: ActorRef<ToDiscovery>,
    discovery_actor_failures: u16,
    actor_namespace: ActorNamespace,
}

pub struct Supervisor;

impl Actor for Supervisor {
    type State = SupervisorState;
    type Msg = ();
    type Arguments = (PrivateKey, Config);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (private_key, config) = args;

        let actor_namespace = generate_actor_namespace(&private_key.public_key());

        // Spawn the events actor.
        let (events_actor, _) = Actor::spawn_linked(
            Some(with_namespace(EVENTS, &actor_namespace)),
            Events,
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the endpoint actor.
        let (endpoint_actor, _) = Actor::spawn_linked(
            Some(with_namespace(ENDPOINT, &actor_namespace)),
            Endpoint,
            (private_key, config.endpoint),
            myself.clone().into(),
        )
        .await?;

        // Spawn the address book actor.
        let (address_book_actor, _) = Actor::spawn_linked(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            AddressBook {},
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the discovery actor.
        let (discovery_actor, _) = Actor::spawn_linked(
            Some(with_namespace(DISCOVERY, &actor_namespace)),
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
            actor_namespace,
        };

        Ok(state)
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
                        "{SUPERVISOR} actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_namespace(EVENTS, &state.actor_namespace) {
                        warn!("{SUPERVISOR} actor: {EVENTS} actor failed: {}", panic_msg);

                        // Respawn the events actor.
                        let (events_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(EVENTS, &state.actor_namespace)),
                            Events {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.events_actor_failures += 1;
                        state.events_actor = events_actor;
                    } else if name == with_namespace(ENDPOINT, &state.actor_namespace) {
                        warn!("{SUPERVISOR} actor: {ENDPOINT} actor failed: {}", panic_msg);

                        // If the endpoint actor fails then the entire system is compromised and we
                        // stop the top-level supervisor actor.
                        myself.stop(Some("{ENDPOINT} actor failed".to_string()));
                    } else if name == with_namespace(ADDRESS_BOOK, &state.actor_namespace) {
                        warn!(
                            "{SUPERVISOR} actor: {ADDRESS_BOOK} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the address book actor.
                        let (address_book_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(ADDRESS_BOOK, &state.actor_namespace)),
                            AddressBook {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.address_book_actor_failures += 1;
                        state.address_book_actor = address_book_actor;
                    } else if name == with_namespace(DISCOVERY, &state.actor_namespace) {
                        warn!(
                            "{SUPERVISOR} actor: {DISCOVERY} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the discovery actor.
                        let (discovery_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(DISCOVERY, &state.actor_namespace)),
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
                        "{SUPERVISOR} actor: {} actor terminated",
                        without_namespace(&name)
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

    use crate::actors::address_book::ADDRESS_BOOK;
    use crate::actors::discovery::DISCOVERY;
    use crate::actors::endpoint::ENDPOINT;
    use crate::actors::events::EVENTS;
    use crate::actors::{generate_actor_namespace, with_namespace};

    use super::{SUPERVISOR, Supervisor};

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn supervisor_child_actors_are_started() {
        let private_key: PrivateKey = Default::default();
        let actor_namespace = generate_actor_namespace(&private_key.public_key());

        let network_config = Default::default();

        let (supervisor_actor, supervisor_actor_handle) = Actor::spawn(
            Some(with_namespace(SUPERVISOR, &actor_namespace)),
            Supervisor,
            (private_key, network_config),
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        supervisor_actor.stop(None);
        supervisor_actor_handle.await.unwrap();

        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {EVENTS} actor"
        )));
        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {ENDPOINT} actor"
        )));
        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {ADDRESS_BOOK} actor"
        )));
        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {DISCOVERY} actor"
        )));

        assert!(!logs_contain("actor failed"));
    }
}
