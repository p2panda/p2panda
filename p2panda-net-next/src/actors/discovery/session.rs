// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub struct DiscoverySessionState {}

#[derive(Default)]
pub struct DiscoverySession;

impl ThreadLocalActor for DiscoverySession {
    type State = DiscoverySessionState;

    type Msg = ();

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(DiscoverySessionState {})
    }
}
