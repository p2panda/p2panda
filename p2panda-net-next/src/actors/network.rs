// SPDX-License-Identifier: MIT OR Apache-2.0

//! Network actor.
//!
//! The root of the entire system supervision tree. It's only role is to spawn and
//! supervise other actors.
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::events::{Events, ToEvents};

pub enum ToNetwork {}

impl Message for ToNetwork {}

pub struct NetworkState {
    events_actor: ActorRef<ToEvents>,
    events_failures: u16,
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
        let (events_actor, _) =
            Actor::spawn_linked(None, Events {}, (), myself.clone().into()).await?;

        let state = NetworkState {
            events_actor,
            events_failures: 0,
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
                if let Some("events") = actor.get_name().as_deref() {
                    warn!("network actor: events actor failed: {}", panic_msg);

                    // Respawn the events actor.
                    let (events_actor, _) =
                        Actor::spawn_linked(None, Events {}, (), myself.clone().into()).await?;

                    state.events_failures += 1;
                    state.events_actor = events_actor;
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
