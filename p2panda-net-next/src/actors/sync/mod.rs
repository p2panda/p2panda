// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

/// Sync manager actor name.
pub const SYNC_MANAGER: &str = "net.sync_manager";

pub enum ToSyncManager {}

#[derive(Default)]
pub struct SyncManager;

impl ThreadLocalActor for SyncManager {
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
