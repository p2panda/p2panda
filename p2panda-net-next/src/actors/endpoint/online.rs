// SPDX-License-Identifier: MIT OR Apache-2.0

//! Online status actor.
//!
//! Waits for the endpoint to have at least one connection to a relay server and one local IP
//! address to be available. This actor is only used for the initial online status check; once it
//! has resolved or a five second timeout has expired, the actor shuts down.
use std::time::Duration;

use iroh::Endpoint;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent, registry};
use tokio::time::timeout;
use tracing::warn;

use crate::actors::events::ToEvents;

pub enum ToOnline {
    WaitUntilConnected,
}

impl Message for ToOnline {}

pub struct OnlineState {
    endpoint: Endpoint,
}

pub struct Online;

impl Actor for Online {
    type State = OnlineState;
    type Msg = ToOnline;
    type Arguments = Endpoint;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        endpoint: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = OnlineState { endpoint };

        // Invoke the handler to wait for the endpoint to be online.
        let _ = myself.cast(ToOnline::WaitUntilConnected);

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
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToOnline::WaitUntilConnected => {
                if timeout(Duration::from_secs(5), state.endpoint.online())
                    .await
                    .is_err()
                {
                    warn!("online actor: failed to connect to relay or receive local ip address")
                }

                // Inform the events actor.
                if let Some(events_actor) = registry::where_is("events".to_string()) {
                    events_actor.send_message(ToEvents::EndpointConnected)?
                }

                // The actor's work is now done.
                myself.stop(Some("online endpoint check is complete".to_string()));

                Ok(())
            }
        }
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }
}
