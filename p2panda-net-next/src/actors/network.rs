// SPDX-License-Identifier: MIT OR Apache-2.0

//! Network actor.
//!
//! The root of the entire system supervision tree; it's only role is to spawn and
//! supervise other actors.
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::address_book::{AddressBook, ToAddressBook};
use crate::actors::discovery::{Discovery, ToDiscovery};
use crate::actors::endpoint::{Endpoint, ToEndpoint};
use crate::actors::events::{Events, ToEvents};

pub enum ToNetwork {}

impl Message for ToNetwork {}

pub struct NetworkState {
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    endpoint_actor: ActorRef<ToEndpoint>,
    endpoint_actor_failures: u16,
    address_book_actor: ActorRef<ToAddressBook>,
    address_book_actor_failures: u16,
    discovery_actor: ActorRef<ToDiscovery>,
    discovery_actor_failures: u16,
}

pub struct Network {}

impl Actor for Network {
    type State = NetworkState;
    type Msg = ToNetwork;
    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Spawn the events actor.
        let (events_actor, _) = Actor::spawn_linked(
            Some("events".to_string()),
            Events {},
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the endpoint actor.
        let (endpoint_actor, _) = Actor::spawn_linked(
            Some("endpoint".to_string()),
            Endpoint {},
            (),
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
            endpoint_actor_failures: 0,
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
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
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

                        // Respawn the endpoint actor.
                        let (endpoint_actor, _) = Actor::spawn_linked(
                            Some("endpoint".to_string()),
                            Endpoint {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.endpoint_actor_failures += 1;
                        state.endpoint_actor = endpoint_actor;
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
