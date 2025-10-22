// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync session actor.
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub enum ToSyncSession {}

pub struct SyncSessionState {}

pub struct SyncSession {}

impl Actor for SyncSession {
    type State = SyncSessionState;
    type Msg = ToSyncSession;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(SyncSessionState {})
    }
}
