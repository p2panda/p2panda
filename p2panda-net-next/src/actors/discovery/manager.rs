// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub const DISCOVERY_MANAGER: &str = "net.discovery.manager";

pub struct DiscoveryManagerState {}

#[derive(Default)]
pub struct DiscoveryManager;

impl ThreadLocalActor for DiscoveryManager {
    type State = DiscoveryManagerState;

    type Msg = ();

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(DiscoveryManagerState {})
    }
}
