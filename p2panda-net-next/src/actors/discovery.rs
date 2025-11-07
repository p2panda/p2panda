// SPDX-License-Identifier: MIT OR Apache-2.0

//! Discovery actor.
use ractor::{
    Actor, ActorProcessingErr, ActorRef, SupervisionEvent, thread_local::ThreadLocalActor,
};

/// Discovery actor name.
pub const DISCOVERY: &str = "net.discovery";

#[derive(Default)]
pub struct Discovery;

impl ThreadLocalActor for Discovery {
    type State = ();
    type Msg = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }
}
