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
    members: &[(ActorId, Access<C>)],
) -> Vec<(GroupMember<ActorId>, Access<C>)> {
    members
        .iter()
        .map(|(member, access)| (typed_member(y, *member), access.clone()))
        .collect()
}

pub(crate) fn sort_members<ID: Ord, C>(members: &mut [(ID, Access<C>)]) {
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
}

pub(crate) fn secret_members<C>(members: &[(ActorId, Access<C>)]) -> Vec<ActorId> {
    let mut members: Vec<ActorId> = members
        .iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(*id) })
        .collect();
    members.sort();
    members
}

pub(crate) fn added_secret_members(
    current_members: &[MemberId],
    next_members: &[MemberId],
) -> Vec<MemberId> {
    let mut members = next_members
        .iter()
        .filter(|actor| !current_members.contains(actor))
        .cloned()
        .collect::<Vec<_>>();
    members.sort();
    members
}

pub(crate) fn removed_secret_members(
    current_members: &[MemberId],
    next_members: &[MemberId],
) -> Vec<MemberId> {
    let mut members = current_members
        .iter()
        .filter(|actor| !next_members.contains(actor))
        .cloned()
        .collect::<Vec<_>>();
    members.sort();
    members
}

pub(crate) fn added_members<C: Clone>(
    current_members: &[(MemberId, Access<C>)],
    next_members: &[(MemberId, Access<C>)],
) -> Vec<(MemberId, Access<C>)> {
    let mut members = next_members
        .iter()
        .filter(|(next_actor, _)| {
            !current_members
                .iter()
                .any(|(current_actor, _)| current_actor == next_actor)
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_members(&mut members);
    members
}

pub(crate) fn promoted_members<C: Clone + PartialOrd>(
    current_members: &[(MemberId, Access<C>)],
    next_members: &[(MemberId, Access<C>)],
) -> Vec<(MemberId, Access<C>)> {
    let mut members = next_members
        .iter()
        .filter(|(next_actor, next_access)| {
            current_members
                .iter()
                .any(|(current_actor, current_access)| {
                    current_actor == next_actor && next_access > current_access
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_members(&mut members);
    members
}

pub(crate) fn demoted_members<C: Clone + PartialOrd>(
    current_members: &[(MemberId, Access<C>)],
    next_members: &[(MemberId, Access<C>)],
) -> Vec<(MemberId, Access<C>)> {
    let mut members = next_members
        .iter()
        .filter(|(next_actor, next_access)| {
            current_members
                .iter()
                .any(|(current_actor, current_access)| {
                    current_actor == next_actor && next_access < current_access
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_members(&mut members);
    members
}

pub(crate) fn removed_members<C: Clone>(
    current_members: &[(MemberId, Access<C>)],
    next_members: &[(MemberId, Access<C>)],
) -> Vec<(MemberId, Access<C>)> {
    let mut members = current_members
        .iter()
        .filter(|(next_actor, _)| {
            !next_members
                .iter()
                .any(|(current_actor, _)| current_actor == next_actor)
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_members(&mut members);
    members
}
