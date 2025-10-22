// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::endpoint::Connecting as IrohConnecting;
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub struct Connection;

impl Actor for Connection {
    type State = ();

    type Msg = ();

    type Arguments = (IrohConnecting,);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }
}
