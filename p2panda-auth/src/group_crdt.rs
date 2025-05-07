// Group membership CRDT functions.

// TODO: Changes that need to be made:
//
// x generic `conditions` parameter on `Write`
// - introduce flag for “any member can add new members” (?)
// - proper error handling
// - tests
// - documentation

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

// TODO: Remove this and replace with custom error type using `thiserror`.
use anyhow::Error;

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum Access<C> {
    Pull,
    Read,
    Write { conditions: C },
    Manage,
}

#[derive(Clone, Debug)]
pub struct MemberState<ID, C> {
    pub member: ID,
    pub member_counter: usize,
    pub access: Access<C>,
    pub access_counter: usize,
}

impl<ID, C> MemberState<ID, C>
where
    C: Clone + Debug + PartialEq,
{
    pub fn is_member(&self) -> bool {
        self.member_counter % 2 != 0
    }

    pub fn is_admin(&self) -> bool {
        self.access == Access::Manage
    }
}

#[derive(Clone, Debug)]
pub struct GroupMembersState<ID, C> {
    pub members: HashMap<ID, MemberState<ID, C>>,
}

impl<ID, C> GroupMembersState<ID, C>
where
    ID: Clone + Hash + Eq,
    C: Clone + Debug + PartialEq,
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

impl<ID, C> Default for GroupMembersState<ID, C>
where
    C: PartialEq,
{
    fn default() -> Self {
        Self {
            members: Default::default(),
        }
    }
}

pub fn create_group<ID: Clone + Eq + Hash, C: Clone + PartialEq>(
    initial_members: &[(ID, Access<C>)],
) -> Result<GroupMembersState<ID, C>, Error> {
    let mut members = HashMap::new();
    for (id, access) in initial_members {
        let member = MemberState {
            member: id.clone(),
            member_counter: 1,
            // TODO: Can we avoid clone here?
            access: access.clone(),
            access_counter: 0,
        };
        members.insert(id.clone(), member);
    }

    let state = GroupMembersState { members };

    Ok(state)
}

pub fn add_member<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq>(
    state: GroupMembersState<ID, C>,
    actor: ID,
    member: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, Error> {
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
                // TODO: Can we avoid clone here?
                state.access = access.clone();
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

pub fn remove_member<ID: Eq + Hash, C: Clone + Debug + PartialEq>(
    state: GroupMembersState<ID, C>,
    actor: ID,
    member: ID,
) -> Result<GroupMembersState<ID, C>, Error> {
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

pub fn promote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    actor: ID,
    member: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, Error> {
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

pub fn demote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    actor: ID,
    member: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, Error> {
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

pub fn merge<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state_1: GroupMembersState<ID, C>,
    state_2: GroupMembersState<ID, C>,
) -> Result<GroupMembersState<ID, C>, Error> {
    // Start from state_2 state.
    let mut next_state = state_2.clone();

    // Iterate over entries in state_1.
    for (id, member_state_1) in state_1.members {
        if let Some(member_state) = next_state.members.get_mut(&id) {
            // If the member is present in both states take the higher counter.
            if member_state_1.member_counter > member_state.member_counter {
                member_state.member_counter = member_state_1.member_counter;
                // TODO: Can we avoid clone here?
                member_state.access = member_state_1.access.clone();
                member_state.access_counter = member_state_1.access_counter;
            }

            // If the member counters are equal, take the access level for the state with a higher
            // access counter. If the access counters are equal, do nothing.
            if member_state_1.member_counter == member_state.member_counter {
                if member_state_1.access_counter > member_state.access_counter {
                    // TODO: Can we avoid clone here?
                    member_state.access = member_state_1.access.clone();
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
