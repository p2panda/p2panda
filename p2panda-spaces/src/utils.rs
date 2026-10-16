// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Conditions;

use crate::store::SpaceState;
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

/// Returns `true` if the given member is part of the members, with any access level.
pub(crate) fn is_member<C>(members: &[(ActorId, Access<C>)], member: ActorId) -> bool {
    members.iter().any(|(id, _)| *id == member)
}

/// Returns `true` if the given member has more than pull access, meaning they can read and decrypt
/// messages.
pub(crate) fn is_secret_member<C>(members: &[(ActorId, Access<C>)], member: ActorId) -> bool {
    members
        .iter()
        .any(|(id, access)| *id == member && !access.is_pull())
}

pub(crate) fn secret_members<C>(members: &[(ActorId, Access<C>)]) -> Vec<ActorId> {
    let mut members: Vec<ActorId> = members
        .iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(*id) })
        .collect();
    members.sort();
    members
}

pub(crate) fn added_secret_members_diff(
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

pub(crate) fn removed_secret_members_diff(
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

pub(crate) fn added_members_diff<C: Clone>(
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

pub(crate) fn promoted_members_diff<C: Clone + PartialOrd>(
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

pub(crate) fn demoted_members_diff<C: Clone + PartialOrd>(
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

pub(crate) fn removed_members_diff<C: Clone>(
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

/// Returns all members who were removed from, and not re-admitted to, the space.
pub(crate) fn removed_space_members<C>(y: &SpaceState<C>) -> Vec<MemberId>
where
    C: Conditions,
{
    let members = y.groups_y.members(y.group_id);
    y.removed
        .iter()
        // Filter out historically removed members which are now current members (they were
        // re-added).
        .filter(|removed| !members.iter().any(|(member, _)| member == *removed))
        .cloned()
        .collect()
}
