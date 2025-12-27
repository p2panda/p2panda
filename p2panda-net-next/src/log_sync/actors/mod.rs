// SPDX-License-Identifier: MIT OR Apache-2.0

mod manager;
mod poller;
mod session;

pub use manager::{SyncManager, ToSyncManager};
use ractor::ActorCell;

use crate::actors::sync::session::{SYNC_SESSION, SyncSessionId};
use crate::actors::{ActorNamespace, with_namespace, without_namespace};

pub const SYNC_PROTOCOL_ID: &[u8] = b"p2panda/sync/v1";

/// Helper to extract information about an actor given it's name (just a string).
#[derive(Debug, PartialEq)]
struct SyncSessionName {
    session_id: SyncSessionId,
}

impl SyncSessionName {
    pub fn new(session_id: SyncSessionId) -> Self {
        Self { session_id }
    }

    fn from_string(name: &str) -> Option<Self> {
        let name = without_namespace(name);
        if name.contains(SYNC_SESSION) {
            Some(Self {
                session_id: Self::extract_id(name),
            })
        } else {
            // The actor is not a sync session.
            None
        }
    }

    pub fn from_actor_cell(actor_cell: &ActorCell) -> Option<Self> {
        if let Some(name) = &actor_cell.get_name() {
            Self::from_string(name)
        } else {
            // The actor is not named.
            None
        }
    }

    fn extract_id(actor_name: &str) -> u64 {
        let Some((_, suffix)) = actor_name.rsplit_once('.') else {
            unreachable!("actors have all the same name pattern")
        };
        suffix.parse::<u64>().expect("suffix is a number")
    }

    pub fn to_string(&self, actor_namespace: &ActorNamespace) -> String {
        with_namespace(
            &format!("{SYNC_SESSION}.{}", self.session_id),
            actor_namespace,
        )
    }
}
