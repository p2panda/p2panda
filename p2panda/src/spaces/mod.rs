// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
mod group;
mod member;
pub(crate) mod message;
mod repair;
mod space;
pub(crate) mod types;

use p2panda_auth::Access;
use p2panda_core::Topic;
use p2panda_store::SqliteStore;

// Re-export useful types.
pub use p2panda_auth::AccessLevel;
pub use p2panda_spaces::manager::ManagerError;
pub use p2panda_spaces::{ActorId, GroupId, GroupsContext, MemberId, SpaceContext, SpaceId};

pub(crate) use forge::{KEY_BUNDLE_LOG_ID, group_log_id};
pub use group::{Group, GroupError, GroupEvent, GroupFuture};
pub use member::{GroupActor, Member, MemberError};
pub(crate) use repair::{RepairError, RepairStrategy, spawn_repair_task};
pub(crate) use space::spaces_stream;
pub use space::{
    AddSpaceMemberError, PublishSpaceError, RemoveSpaceMemberError, Space, SpaceFuture,
    SpaceSubscription,
};
pub use types::{InnerGroupEvent, SpacesManagerError};

use crate::Credentials;
use crate::forge::OperationForge;
use crate::spaces::types::{AuthCapabilities, SpacesManager, SpacesStore};

#[allow(clippy::result_large_err)]
pub fn spaces_manager(
    forge: OperationForge,
    credentials: Credentials,
    store: SqliteStore,
) -> Result<SpacesManager, SpacesManagerError> {
    use p2panda_encryption::Rng;

    use crate::spaces::types::SpacesManager;

    let rng = Rng::default();
    let spaces_store = SpacesStore::new(store.clone());

    SpacesManager::new(spaces_store, forge, (&credentials).into(), rng)
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
                    level: *level,
                },
            )
        })
        .collect()
}

pub(crate) fn to_members(
    members: &[(ActorId, Access<AuthCapabilities>)],
) -> Vec<(ActorId, AccessLevel)> {
    members
        .iter()
        .map(|(actor, access)| (*actor, access.level))
        .collect()
}

pub(crate) fn to_actors(
    actors: &[(p2panda_spaces::GroupActor, Access<AuthCapabilities>)],
) -> Vec<(GroupActor, AccessLevel)> {
    actors
        .iter()
        .map(|(actor, access)| (actor.clone().into(), access.level))
        .collect()
}
