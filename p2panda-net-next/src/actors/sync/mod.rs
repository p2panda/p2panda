// SPDX-License-Identifier: MIT OR Apache-2.0

mod manager;
mod session;
mod poller;

// @TODO: uncomment when we plyg in the actual sync actor.
// pub use manager::{SYNC_MANAGER, SyncManager, ToSyncManager};

pub const SYNC_PROTOCOL_ID: &[u8] = b"p2panda/sync/v1";

// @TODO: remove when we plug in the actual sync actor.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

/// Sync manager actor name.
pub const SYNC_MANAGER: &str = "net.sync_manager";

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