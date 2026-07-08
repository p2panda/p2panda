// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Conditions;

use crate::types::AuthGroupState;
use crate::{ActorId, MemberId};

/// Assign a GroupMember type to passed actor based on looking up if the actor is a group in the
/// auth state.
pub(crate) fn typed_member<C: Conditions>(
    y: &AuthGroupState<C>,
    member: ActorId,
) -> GroupMember<ActorId> {
    if !y.groups_global().contains(&member) {
        GroupMember::Individual(member)
    } else {
        GroupMember::Group(member)
    }
}

/// Assign GroupMember type to every actor based on looking up if the actor is a group in the auth
/// state.
pub(crate) fn typed_members<C: Conditions>(
    y: &AuthGroupState<C>,
    members: Vec<(ActorId, Access<C>)>,
) -> Vec<(GroupMember<ActorId>, Access<C>)> {
    members
        .into_iter()
        .map(|(member, access)| (typed_member(y, member), access))
        .collect()
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
    current_members: Vec<MemberId>,
    next_members: Vec<MemberId>,
) -> Vec<MemberId> {
    let mut members = next_members
        .into_iter()
        .filter(|actor| !current_members.contains(actor))
        .collect::<Vec<_>>();
    members.sort();
    members
}

pub(crate) fn removed_members(
    current_members: Vec<MemberId>,
    next_members: Vec<MemberId>,
) -> Vec<MemberId> {
    let mut members = current_members
        .into_iter()
        .filter(|actor| !next_members.contains(actor))
        .collect::<Vec<_>>();
    members.sort();
    members
}
