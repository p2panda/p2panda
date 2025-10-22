// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::endpoint::Connection as IrohConnection;
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub struct DiscoverySessionState {}

pub struct DiscoverySession;

impl Actor for DiscoverySession {
    type State = DiscoverySessionState;

    type Msg = ();

    type Arguments = (IrohConnection,);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(DiscoverySessionState {})
    }
}
