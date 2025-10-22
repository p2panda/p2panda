// SPDX-License-Identifier: MIT OR Apache-2.0

mod listener;
mod receiver;
mod sender;
mod session;

use ractor::{Actor, ActorProcessingErr, ActorRef};

pub enum ToSync {}

pub struct SyncState {}

pub struct Sync;

impl Actor for Sync {
    type State = SyncState;
    type Msg = ToSync;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(SyncState {})
    }
}
