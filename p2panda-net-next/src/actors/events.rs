// SPDX-License-Identifier: MIT OR Apache-2.0

//! Events actor.
//!
//! Receives events from other actors, aggregating and enriching them before informing
//! upstream subscribers.
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::debug;

pub enum ToEvents {
    EndpointConnected,
}

impl Message for ToEvents {}

pub struct EventsState {}

pub struct Events;

impl Actor for Events {
    type State = EventsState;
    type Msg = ToEvents;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(EventsState {})
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
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            // TODO: Eventually we want to enrich and forward events to the subscription actor so
            // they can be passed to any subscribers.
            ToEvents::EndpointConnected => debug!("endpoint connected to relay"),
        }

        Ok(())
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
