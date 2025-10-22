// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::{Actor, ActorProcessingErr, ActorRef};

pub const NETWORK: &str = "net.network";

pub enum ToNetwork {}

pub struct NetworkState {}

pub struct Network;

impl Actor for Network {
    type State = NetworkState;

    type Msg = ToNetwork;

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(NetworkState {})
    }
}
