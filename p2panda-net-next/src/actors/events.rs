// SPDX-License-Identifier: MIT OR Apache-2.0

//! Events actor.
//!
//! Receives events from other actors, aggregating and enriching them before informing
//! upstream subscribers.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

/// Events actor name.
pub const EVENTS: &str = "net.events";

pub enum ToEvents {}

#[derive(Default)]
pub struct Events;

impl ThreadLocalActor for Events {
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
}
