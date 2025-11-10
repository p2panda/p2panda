// SPDX-License-Identifier: MIT OR Apache-2.0


mod manager;
mod session;

pub use manager::{SYNC_MANAGER, SyncManager, ToSyncManager};

pub const SYNC_PROTOCOL_ID: &[u8] = b"p2panda/sync/v1";

// @TODO: remove all of this when all other actors using sync are updated.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

#[derive(Default)]
pub struct SyncManager;

impl ThreadLocalActor for SyncManager {
    type State = ();
    type Msg = ToSyncManager;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }
}
