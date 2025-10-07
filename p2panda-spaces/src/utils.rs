// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::{SystemTime, UNIX_EPOCH};

use p2panda_auth::traits::Conditions;
use p2panda_auth::{Access, group::GroupMember};

use crate::manager::Manager;
use crate::traits::spaces_store::AuthStore;
use crate::{ActorId, types::AuthGroupState};

/// Assign a GroupMember type to passed actor based on looking up if the actor is a group in the
/// auth state.
pub(crate) fn typed_member<C: Conditions>(
    auth_y: &AuthGroupState<C>,
    member: ActorId,
) -> GroupMember<ActorId> {
    if auth_y.members(member).is_empty() {
        GroupMember::Individual(member)
    } else {
        GroupMember::Group(member)
    }
}

/// Assign GroupMember type to every actor based on looking up if the actor is a group in the auth
/// state.
pub(crate) async fn typed_members<ID, S, F, M, C, RS>(
    manager_ref: Manager<ID, S, F, M, C, RS>,
    members: Vec<(ActorId, Access<C>)>,
) -> Result<Vec<(GroupMember<ActorId>, Access<C>)>, <S as AuthStore<C>>::Error>
where
    S: AuthStore<C>,
    C: Conditions,
{
    let manager = manager_ref.inner.read().await;
    let auth_y = manager.spaces_store.auth().await?;
    Ok(members
        .into_iter()
        .map(|(member, access)| (typed_member(&auth_y, member), access))
        .collect())
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
}
