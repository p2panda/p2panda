// State-based CL-CRDT for maintaining group memberships with associated access levels.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::traits::IdentityHandle;

use super::access::Access;

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupMembershipError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberState<ID> {
    pub member: ID,
    pub member_counter: usize,
    pub access: Access,
    pub access_counter: usize,
}

impl<ID> MemberState<ID> {
    pub fn is_member(&self) -> bool {
        self.member_counter % 2 != 0
    }

    pub fn is_admin(&self) -> bool {
        self.access == Access::Manage
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMembersState<ID>
where
    ID: IdentityHandle,
{
    pub members: HashMap<ID, MemberState<ID>>,
}

impl<ID> GroupMembersState<ID>
where
    ID: IdentityHandle,
{
    pub fn members(&self) -> HashSet<ID> {
        self.members
            .values()
            .filter_map(|state| {
                if state.is_member() {
                    Some(state.member.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>()
    }

    pub fn admins(&self) -> HashSet<ID> {
        self.members
            .values()
            .filter_map(|state| {
                if state.is_admin() && state.is_member() {
                    Some(state.member.clone())
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>()
    }
}

impl<ID> Default for GroupMembersState<ID>
where
    ID: IdentityHandle,
{
    fn default() -> Self {
        Self {
            members: Default::default(),
        }
    }
}

pub fn create_group<ID: IdentityHandle>(
    initial_members: &[(ID, Access)],
) -> Result<GroupMembersState<ID>, GroupMembershipError> {
    let mut members = HashMap::new();
    for (id, access) in initial_members {
        let member = MemberState {
            member: id.clone(),
            member_counter: 1,
            access: *access,
            access_counter: 0,
        };
        members.insert(id.clone(), member);
    }

    let state = GroupMembersState { members };

    Ok(state)
}

pub fn add_member<ID: IdentityHandle>(
    state: GroupMembersState<ID>,
    actor: ID,
    member: ID,
    access: Access,
) -> Result<GroupMembersState<ID>, GroupMembershipError> {
    // TODO: Consider whether we want to return Error rather than the unchanged state...
    // The error would communicate why there was an early return.

    // Check the actor is known to the group.
    let Some(actor) = state.members.get(&actor) else {
        // Throw error here.
        panic!()
    };

    // If "actor" is not a current group member or not an admin, do not perform the add and
    // directly return the state.
    if !actor.is_member() || !actor.is_admin() {
        panic!()
    };

    // Add "member" to the group or increment their counter if they are already known but were
    // previously removed.
    let mut state = state;
    state
        .members
        .entry(member.clone())
        .and_modify(|state| {
            if !state.is_member() {
                state.member_counter += 1;
                state.access = access;
                state.access_counter = 0;
            }
        })
        .or_insert(MemberState {
            member,
            member_counter: 1,
            access,
            access_counter: 0,
        });

    Ok(state)
}

pub fn remove_member<ID: IdentityHandle>(
    state: GroupMembersState<ID>,
    actor: ID,
    member: ID,
) -> Result<GroupMembersState<ID>, GroupMembershipError> {
    // Check "actor" is known to the group.
    let Some(actor) = state.members.get(&actor) else {
        return Ok(state);
    };

    // If "actor" is not a current group member or not an admin, do not perform the remove and
    // directly return the state.
    if !actor.is_member() || !actor.is_admin() {
        panic!()
    };

    // Check "member" is in the group.
    if state.members.get(&member).is_none() {
        panic!()
    };

    // Increment "member" counter unless they are already removed.
    let mut state = state;
    state.members.entry(member).and_modify(|state| {
        if state.is_member() {
            state.member_counter += 1;
            state.access_counter = 0;
        }
    });

    Ok(state)
}

pub fn promote<ID: IdentityHandle>(
    state: GroupMembersState<ID>,
    actor: ID,
    member: ID,
    access: Access,
) -> Result<GroupMembersState<ID>, GroupMembershipError> {
    // Check "actor" is known to the group.
    let Some(actor) = state.members.get(&actor) else {
        panic!()
    };

    // If "actor" is not a current group member or not an admin, do not perform the remove and
    // directly return the state.
    if !actor.is_member() || !actor.is_admin() {
        panic!()
    };

    // Check "member" is in the group.
    if state.members.get(&member).is_none() {
        panic!()
    };

    // Update access level.
    let mut state = state;
    state.members.entry(member).and_modify(|state| {
        if state.access < access {
            state.access = access;
            state.access_counter += 1;
        }
    });

    Ok(state)
}

pub fn demote<ID: IdentityHandle>(
    state: GroupMembersState<ID>,
    actor: ID,
    member: ID,
    access: Access,
) -> Result<GroupMembersState<ID>, GroupMembershipError> {
    // Check "actor" is known to the group.
    let Some(actor) = state.members.get(&actor) else {
        panic!()
    };

    // If "actor" is not a current group member or not an admin, do not perform the remove and
    // directly return the state.
    if !actor.is_member() || !actor.is_admin() {
        panic!()
    };

    // Check "member" is in the group.
    if state.members.get(&member).is_none() {
        panic!()
    };

    // Update access level.
    let mut state = state;
    state.members.entry(member).and_modify(|state| {
        if state.access > access {
            state.access = access;
            state.access_counter += 1;
        }
    });

    Ok(state)
}

pub fn merge<ID: IdentityHandle>(
    state_1: GroupMembersState<ID>,
    state_2: GroupMembersState<ID>,
) -> Result<GroupMembersState<ID>, GroupMembershipError> {
    // Start from state_2 state.
    let mut next_state = state_2.clone();

    // Iterate over entries in state_1.
    for (id, member_state_1) in state_1.members {
        if let Some(member_state) = next_state.members.get_mut(&id) {
            // If the member is present in both states take the higher counter.
            if member_state_1.member_counter > member_state.member_counter {
                member_state.member_counter = member_state_1.member_counter;
                member_state.access = member_state_1.access;
                member_state.access_counter = member_state_1.access_counter;
            }

            // If the member counters are equal, take the access level for the state with a higher
            // access counter. If the access counters are equal, do nothing.
            if member_state_1.member_counter == member_state.member_counter {
                if member_state_1.access_counter > member_state.access_counter {
                    member_state.access = member_state_1.access;
                    member_state.access_counter = member_state_1.access_counter;
                }

                // If the access counters are the same, take the lower of the two access levels.
                if member_state_1.access_counter == member_state.access_counter {
                    if member_state_1.access < member_state.access {
                        member_state.access = member_state_1.access;
                    }
                }
            }
        } else {
            // Otherwise insert the member into the next state.
            next_state.members.insert(id, member_state_1);
        }
    }

    Ok(next_state)
}
