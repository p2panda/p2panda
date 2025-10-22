// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::{Actor, ActorProcessingErr, ActorRef};

pub struct SyncSender;

impl Actor for SyncSender {
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
