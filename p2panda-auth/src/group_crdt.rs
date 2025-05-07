// Group membership CRDT functions.

// TODO: Changes that need to be made:
//
// x generic `conditions` parameter on `Write`
// - introduce flag for “any member can add new members” (?)
// x proper error handling
// - tests
// - documentation

// Glossary
//
// - actor: an entity performing a group action (eg. adding or removing a member from a group)
// - member: an entity on which a group action is performed (e.g. added or removed from a group)

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GroupMembershipError {
    #[error("tried to add a member who is already active in the group")]
    AlreadyAdded,

    #[error("tried to remove a member who is already inactive in the group")]
    AlreadyRemoved,

    #[error("actor lacks sufficient access to update the group")]
    InsufficientAccess,

    #[error("actor is not an active member of the group")]
    InactiveActor,

    #[error("actor is not known to the group")]
    UnrecognisedActor,

    #[error("member is not known to the group")]
    UnrecognisedMember,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum Access<C> {
    Pull,
    Read,
    Write { conditions: Option<C> },
    Manage,
}

#[derive(Clone, Debug)]
pub struct MemberState<C> {
    pub member_counter: usize,
    pub access: Access<C>,
    pub access_counter: usize,
}

impl<C> MemberState<C>
where
    C: Clone + Debug + PartialEq,
{
    pub fn is_member(&self) -> bool {
        self.member_counter % 2 != 0
    }

    pub fn is_manager(&self) -> bool {
        self.access == Access::Manage
    }
}

#[derive(Clone, Debug)]
pub struct GroupMembersState<ID, C> {
    pub members: HashMap<ID, MemberState<C>>,
}

impl<ID, C> GroupMembersState<ID, C>
where
    ID: Clone + Hash + Eq,
    C: Clone + Debug + PartialEq,
{
    pub fn members(&self) -> HashSet<ID> {
        self.members
            .iter()
            .filter_map(|(id, state)| {
                if state.is_member() {
                    Some(id.to_owned())
                } else {
                    None
                }
            })
            .collect::<HashSet<ID>>()
    }

    pub fn managers(&self) -> HashSet<ID> {
        self.members
            .iter()
            .filter_map(|(id, state)| {
                if state.is_member() && state.is_manager() {
                    Some(id.to_owned())
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
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    let mut members = HashMap::new();
    for (id, access) in initial_members {
        let member = MemberState {
            member_counter: 1,
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
    adder: ID,
    added: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Ensure that "adder" is known to the group.
    let Some(adder) = state.members.get(&adder) else {
        return Err(GroupMembershipError::UnrecognisedActor);
    };

    // Ensure that "adder" is a member of the group with manage access level.
    if !adder.is_member() {
        return Err(GroupMembershipError::InactiveActor);
    } else if !adder.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess);
    }

    // Ensure that "added" is not already an active member of the group.
    if let Some(added) = state.members.get(&added) {
        if added.is_member() {
            return Err(GroupMembershipError::AlreadyAdded);
        }
    }

    // Add "added" to the group or increment their counters if they are already known but were
    // previously removed.
    let mut state = state;
    state
        .members
        .entry(added.clone())
        .and_modify(|added| {
            if !added.is_member() {
                added.member_counter += 1;
                added.access = access.clone();
                added.access_counter = 0;
            }
        })
        .or_insert(MemberState {
            member_counter: 1,
            access,
            access_counter: 0,
        });

    Ok(state)
}

pub fn remove_member<ID: Eq + Hash, C: Clone + Debug + PartialEq>(
    state: GroupMembersState<ID, C>,
    remover: ID,
    removed: ID,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Ensure that "remover" is known to the group.
    let Some(remover) = state.members.get(&remover) else {
        return Err(GroupMembershipError::UnrecognisedActor);
    };

    // Ensure that "remover" is a member of the group with manage access level.
    if !remover.is_member() {
        return Err(GroupMembershipError::InactiveActor);
    } else if !remover.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess);
    }

    // Ensure that "removed" is known to the group.
    if !state.members.contains_key(&removed) {
        return Err(GroupMembershipError::UnrecognisedMember);
    };

    // Ensure that "removed" is not already an inactive member of the group.
    if let Some(removed) = state.members.get(&removed) {
        if !removed.is_member() {
            return Err(GroupMembershipError::AlreadyRemoved);
        }
    }

    // Increment "removed" counters unless they are already removed.
    let mut state = state;
    state.members.entry(removed).and_modify(|removed| {
        if removed.is_member() {
            removed.member_counter += 1;
            removed.access_counter = 0;
        }
    });

    Ok(state)
}

pub fn promote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    promoter: ID,
    promoted: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Ensure that "promoter" is known to the group.
    let Some(promoter) = state.members.get(&promoter) else {
        return Err(GroupMembershipError::UnrecognisedActor);
    };

    // Ensure that "promoter" is a member of the group with manage access level.
    if !promoter.is_member() {
        return Err(GroupMembershipError::InactiveActor);
    } else if !promoter.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess);
    }

    // Ensure that "promoted" is known to the group.
    if !state.members.contains_key(&promoted) {
        return Err(GroupMembershipError::UnrecognisedMember);
    };

    // Update access level.
    let mut state = state;
    state.members.entry(promoted).and_modify(|promoted| {
        if promoted.access < access {
            promoted.access = access;
            promoted.access_counter += 1;
        }
    });

    Ok(state)
}

pub fn demote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    demoter: ID,
    demoted: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Ensure that "demoter" is known to the group.
    let Some(demoter) = state.members.get(&demoter) else {
        return Err(GroupMembershipError::UnrecognisedActor);
    };

    // Ensure that "demoter" is a member of the group with manage access level.
    if !demoter.is_member() {
        return Err(GroupMembershipError::InactiveActor);
    } else if !demoter.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess);
    }

    // Ensure that "demoted" is known to the group.
    if !state.members.contains_key(&demoted) {
        return Err(GroupMembershipError::UnrecognisedMember);
    };

    // Update access level.
    let mut state = state;
    state.members.entry(demoted).and_modify(|demoted| {
        if demoted.access > access {
            demoted.access = access;
            demoted.access_counter += 1;
        }
    });

    Ok(state)
}

pub fn merge<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state_1: GroupMembersState<ID, C>,
    state_2: GroupMembersState<ID, C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Start from state_2 state.
    let mut next_state = state_2.clone();

    // Iterate over entries in state_1.
    for (id, member_state_1) in state_1.members {
        if let Some(member_state) = next_state.members.get_mut(&id) {
            // If the member is present in both states take the higher counter.
            if member_state_1.member_counter > member_state.member_counter {
                member_state.member_counter = member_state_1.member_counter;
                member_state.access = member_state_1.access.clone();
                member_state.access_counter = member_state_1.access_counter;
            }

            // If the member counters are equal, take the access level for the state with a higher
            // access counter. If the access counters are equal, do nothing.
            if member_state_1.member_counter == member_state.member_counter {
                if member_state_1.access_counter > member_state.access_counter {
                    member_state.access = member_state_1.access.clone();
                    member_state.access_counter = member_state_1.access_counter;
                }

                // If the access counters are the same, take the lower of the two access levels.
                if member_state_1.access_counter == member_state.access_counter
                    && member_state_1.access < member_state.access
                {
                    member_state.access = member_state_1.access;
                }
            }
        } else {
            // Otherwise insert the member into the next state.
            next_state.members.insert(id, member_state_1);
        }
    }

    Ok(next_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_add_remove() {
        // "Happy path" test for create, add and remove functions.

        // TODO: Is there a way to avoid this completely?
        //
        // Avoid having to annotate the conditions `C`.
        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [(alice, AccessLevel::Manage), (bob, AccessLevel::Read)];

        // Alice create a group with Alice and Bob as members.
        let group_y = create_group(&initial_members).unwrap();

        assert!(group_y.members().contains(&alice));
        assert!(group_y.members().contains(&bob));

        assert!(group_y.managers().contains(&alice));
        assert!(!group_y.managers().contains(&bob));

        // Alice adds Charlie.
        let group_y = add_member(
            group_y,
            alice,
            charlie,
            AccessLevel::Write {
                conditions: Some("requirement".to_string()),
            },
        )
        .unwrap();

        assert!(group_y.members().contains(&charlie));

        // Alice removes Bob.
        let group_y = remove_member(group_y, alice, bob).unwrap();

        assert!(!group_y.members().contains(&bob));
    }
}
