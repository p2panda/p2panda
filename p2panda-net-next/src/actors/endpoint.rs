// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint actor.
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::router::{Router, ToRouter};

// TODO: Remove once used.
#[allow(dead_code)]
pub struct EndpointConfig {}

pub enum ToEndpoint {}

impl Message for ToEndpoint {}

pub struct EndpointState {
    router_actor: ActorRef<ToRouter>,
    router_failures: u16,
}

pub struct Endpoint {}

impl Actor for Endpoint {
    type State = EndpointState;
    type Msg = ToEndpoint;
    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Spawn the router actor.
        let (router_actor, _) =
            Actor::spawn_linked(Some("router".to_string()), Router {}, (), myself.into()).await?;

        let state = EndpointState {
            router_actor,
            router_failures: 0,
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
                    debug!("endpoint actor: received ready from {} actor", name);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some("router") = actor.get_name().as_deref() {
                    warn!("network actor: router actor failed: {}", panic_msg);

                    // Respawn the router actor.
                    let (router_actor, _) = Actor::spawn_linked(
                        Some("router".to_string()),
                        Router {},
                        (),
                        myself.clone().into(),
                    )
                    .await?;

                    state.router_failures += 1;
                    state.router_actor = router_actor;
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
