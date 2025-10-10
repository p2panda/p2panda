// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Conditions;

use crate::ActorId;
use crate::manager::Manager;
use crate::traits::AuthStore;
use crate::types::AuthGroupState;

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
pub(crate) async fn typed_members<ID, S, K, F, M, C, RS>(
    manager_ref: Manager<ID, S, K, F, M, C, RS>,
    members: Vec<(ActorId, Access<C>)>,
) -> Result<Vec<(GroupMember<ActorId>, Access<C>)>, <S as AuthStore<C>>::Error>
where
    S: AuthStore<C>,
    C: Conditions,
{
    let manager = manager_ref.inner.read().await;
    let auth_y = manager.store.auth().await?;
    Ok(members
        .into_iter()
        .map(|(member, access)| (typed_member(&auth_y, member), access))
        .collect())
}

pub(crate) fn sort_members<ID: Ord, C>(members: &mut [(ID, Access<C>)]) {
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
}

pub(crate) fn secret_members<C>(members: Vec<(ActorId, Access<C>)>) -> Vec<ActorId> {
    let mut members: Vec<ActorId> = members
        .into_iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(id) })
        .collect();
    members.sort();
    members
}

pub(crate) fn added_members(
    current_members: Vec<ActorId>,
    next_members: Vec<ActorId>,
) -> Vec<ActorId> {
    let mut members = next_members
        .iter()
        .cloned()
        .filter(|actor| !current_members.contains(actor))
        .collect::<Vec<_>>();
    members.sort();
    members
}

pub(crate) fn removed_members(
    current_members: Vec<ActorId>,
    next_members: Vec<ActorId>,
) -> Vec<ActorId> {
    let mut members = current_members
        .iter()
        .cloned()
        .filter(|actor| !next_members.contains(actor))
        .collect::<Vec<_>>();
    members.sort();
    members
}
