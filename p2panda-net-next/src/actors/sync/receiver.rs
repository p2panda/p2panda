// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync receiver actor.
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub enum ToSyncReceiver {}

pub struct SyncReceiverState {}

pub struct SyncReceiver {}

impl Actor for SyncReceiver {
    type State = SyncReceiverState;
    type Msg = ToSyncReceiver;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(SyncReceiverState {})
    }
}
