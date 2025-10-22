// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::{Actor, ActorProcessingErr, ActorRef};

pub const DISCOVERY: &str = "net.discovery";

pub const DISCOVERY_PROTOCOL_ID: &[u8] = b"p2panda/discovery/v1";

pub enum ToDiscovery {}

pub struct DiscoveryState {}

pub struct Discovery;

impl Actor for Discovery {
    type State = DiscoveryState;

    type Msg = ToDiscovery;

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(DiscoveryState {})
    }
}
