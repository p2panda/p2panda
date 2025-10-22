// SPDX-License-Identifier: MIT OR Apache-2.0

//! Events actor.
//!
//! Receives events from other actors, aggregating and enriching them before informing
//! upstream subscribers.
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::debug;

pub const EVENTS: &str = "net.events";

pub enum ToEvents {
    ConnectedToRelay,
}

pub struct Events;

impl Actor for Events {
    type State = ();

    type Msg = ToEvents;

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            // @TODO: Eventually we want to enrich and forward events to the subscription actor so
            // they can be passed to any subscribers.
            ToEvents::ConnectedToRelay => debug!("endpoint connected to relay"),
        }

        Ok(())
    }
}
