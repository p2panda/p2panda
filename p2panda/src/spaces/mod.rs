// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
mod group;
mod member;
pub(crate) mod message;
mod space;
pub(crate) mod types;

use p2panda_auth::Access;
use p2panda_core::{Topic, VerifyingKey};
use p2panda_store::SqliteStore;

// Re-export useful types.
pub use p2panda_auth::AccessLevel;
pub use p2panda_spaces::manager::ManagerError;
pub use p2panda_spaces::{ActorId, GroupContext, GroupId, MemberId, SpaceContext, SpaceId};

pub(crate) use forge::KEY_BUNDLE_LOG_ID;
pub use group::{Group, GroupError, GroupEvent, GroupFuture};
pub use member::{GroupActor, Member, MemberError};
pub(crate) use space::spaces_stream;
pub use space::{Space, SpaceError, SpaceEvent, SpaceFuture, SpaceSubscription};
pub use types::SpacesManagerError;

use crate::Credentials;
use crate::forge::OperationForge;
use crate::operation::LogId;
use crate::spaces::forge::GROUP_CONTROL_MESSAGE;
use crate::spaces::types::{AuthCapabilities, SpacesManager, SpacesStore};

pub async fn spaces_manager(
    forge: OperationForge,
    credentials: Credentials,
    store: SqliteStore,
) -> Result<SpacesManager, SpacesManagerError> {
    use p2panda_encryption::Rng;

    use crate::spaces::types::SpacesManager;

    let rng = Rng::default();
    let spaces_store = SpacesStore::new(store.clone());

    SpacesManager::new(spaces_store, forge, (&credentials).into(), rng).await
}

pub(crate) fn actor_to_topic(actor_id: impl Into<ActorId>) -> Topic {
    actor_id.into().as_bytes().to_owned().into()
}

pub(crate) fn to_initial_members(
    initial_members: &[(ActorId, AccessLevel)],
) -> Vec<(ActorId, Access<AuthCapabilities>)> {
    initial_members
        .iter()
        .map(|(actor, level)| {
            (
                *actor,
                Access {
                    conditions: None,
                    level: level.clone(),
                },
            )
        })
        .collect()
}

pub(crate) fn group_log_id(group_id: VerifyingKey) -> LogId {
    LogId::digest(&{
        let mut bytes = Vec::new();
        bytes.extend_from_slice(group_id.as_bytes());
        // The group id would be enough to indicate the log id, we hash it here together
        // with a constant value to prevent possible collisions with logs of same id but
        // different purpose.
        bytes.extend_from_slice(GROUP_CONTROL_MESSAGE);
        bytes
    })
}
