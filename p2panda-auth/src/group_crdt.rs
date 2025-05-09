// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group membership CRDT.

// TODO: Changes that need to be made:
//
// - introduce flag for “any member can add new members” (?)
//   - need to think about behaviour when it comes to `promote` and `demote`

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum GroupMembershipError {
    #[error("attempted to add a member who is already active in the group")]
    AlreadyAdded,

    #[error("attempted to remove a member who is already inactive in the group")]
    AlreadyRemoved,

    #[error("actor lacks sufficient access to update the group")]
    InsufficientAccess,

    #[error("actor is not an active member of the group")]
    InactiveActor,

    #[error("member is not an active member of the group")]
    InactiveMember,

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
    member_counter: usize,
    access: Access<C>,
    access_counter: usize,
}

impl<C> MemberState<C>
where
    C: Clone + Debug + PartialEq,
{
    pub fn is_member(&self) -> bool {
        self.member_counter % 2 != 0
    }

    pub fn is_puller(&self) -> bool {
        self.access == Access::Pull
    }

    pub fn is_reader(&self) -> bool {
        self.access == Access::Read
    }

    pub fn is_writer(&self) -> bool {
        self.access != Access::Pull && self.access != Access::Read && self.access != Access::Manage
    }

    pub fn is_manager(&self) -> bool {
        self.access == Access::Manage
    }
}

#[derive(Clone, Debug)]
pub struct GroupMembersState<ID, C> {
    members: HashMap<ID, MemberState<C>>,
}

impl<ID, C> GroupMembersState<ID, C>
where
    ID: Clone + Hash + Eq,
    C: Clone + Debug + PartialEq,
{
    /// Return all active group members.
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

    /// Return all active group members with manager access.
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

/// Create a new group and add the given set of initial members.
pub fn create<ID: Clone + Eq + Hash, C: Clone + PartialEq>(
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

/// Add a member to the group with the given access level.
///
/// The actor performing the action must be an active member of the group with manager access.
/// Re-adding a previously removed member is supported.
pub fn add<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq>(
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

/// Remove a member from the group.
///
/// The actor performing the action must be an active member of the group with manager access.
pub fn remove<ID: Eq + Hash, C: Clone + Debug + PartialEq>(
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

/// Modify the access level of a group member.
///
/// The actor performing the action must be an active member of the group with manager access.
pub fn modify<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    modifier: ID,
    modified: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Ensure that "modifier" is known to the group.
    let Some(modifier) = state.members.get(&modifier) else {
        return Err(GroupMembershipError::UnrecognisedActor);
    };

    // Ensure that "modifier" is a member of the group with manage access level.
    if !modifier.is_member() {
        return Err(GroupMembershipError::InactiveActor);
    } else if !modifier.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess);
    }

    // Ensure that "modified" is an active member of the group.
    if let Some(modified) = state.members.get(&modified) {
        if !modified.is_member() {
            return Err(GroupMembershipError::InactiveMember);
        }
    } else {
        return Err(GroupMembershipError::UnrecognisedMember);
    }

    // Update access level.
    let mut state = state;
    state.members.entry(modified).and_modify(|modified| {
        // Only perform the modification if the access levels differ.
        if modified.access != access {
            modified.access = access;
            modified.access_counter += 1;
        }
    });

    Ok(state)
}

/// Promote a group member by a single access level.
///
/// No modification will occur if the promoted member already has `Manage` access. In that case, the
/// given state is returned unchanged.
///
/// Conditions may be optionally provided. These are only applied in the case of a promotion from
/// `Read` to `Write`.
///
/// The actor performing the action must be an active member of the group with manager access.
pub fn promote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    promoter: ID,
    promoted: ID,
    conditions: Option<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    if let Some(member) = state.members.get(&promoted) {
        let new_state = match member.access {
            Access::Pull => modify(state, promoter, promoted, Access::Read)?,
            Access::Read => modify(state, promoter, promoted, Access::Write { conditions })?,
            Access::Write { .. } => modify(state, promoter, promoted, Access::Manage)?,
            Access::Manage => state,
        };

        Ok(new_state)
    } else {
        Err(GroupMembershipError::UnrecognisedMember)
    }
}

/// Demote a group member by a single access level.
///
/// No modification will occur if the demoted member already has `Pull` access. In that case, the
/// given state is returned unchanged.
///
/// Conditions may be optionally provided. These are only applied in the case of a demotion from
/// `Manage` to `Write`.
///
/// The actor performing the action must be an active member of the group with manager access.
pub fn demote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    demoter: ID,
    demoted: ID,
    conditions: Option<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    if let Some(member) = state.members.get(&demoted) {
        let new_state = match member.access {
            Access::Pull => state,
            Access::Read => modify(state, demoter, demoted, Access::Pull)?,
            Access::Write { .. } => modify(state, demoter, demoted, Access::Read)?,
            Access::Manage => modify(state, demoter, demoted, Access::Write { conditions })?,
        };

        Ok(new_state)
    } else {
        Err(GroupMembershipError::UnrecognisedMember)
    }
}

/// Merge two group states into one using a deterministic, conflict-free approach.
///
/// Grow-only counters are used internally to track state changes; one counter for add / remove
/// actions and one for access modification action. These values are used to determine which
/// membership and access states should be included in the merged group state. A state with a higher
/// counter indicates that it has undergone more actions; this state will be included in the merge.
///
/// If a member exists with different access levels in each state and has undergone the same number
/// of access modifications, the lower of the two access levels will be chosen.
pub fn merge<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state_1: GroupMembersState<ID, C>,
    state_2: GroupMembersState<ID, C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError> {
    // Start from state_2 state.
    let mut next_state = state_2.clone();

    // Iterate over entries in state_1.
    for (id, member_state_1) in state_1.members {
        if let Some(member_state) = next_state.members.get_mut(&id) {
            // If the member is present in both states, take the higher counter.
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

        // @TODO(glyph): Is there a way to avoid this completely?
        //
        // Avoid having to annotate the conditions `C`.
        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [(alice, AccessLevel::Manage), (bob, AccessLevel::Read)];

        // Alice creates a group with Alice and Bob as members.
        let group_y = create(&initial_members).unwrap();

        assert!(group_y.members().contains(&alice));
        assert!(group_y.members().contains(&bob));

        assert!(group_y.managers().contains(&alice));
        assert!(!group_y.managers().contains(&bob));

        // Alice adds Charlie.
        let group_y = add(
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
        let group_y = remove(group_y, alice, bob).unwrap();

        assert!(!group_y.members().contains(&bob));
    }

    #[test]
    fn promote_demote_modify() {
        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;

        let initial_members = [(alice, AccessLevel::Manage), (bob, AccessLevel::Read)];

        // Alice creates a group with Alice and Bob as members.
        let group_y = create(&initial_members).unwrap();

        // Alice promotes Bob to Write access.
        let group_y = promote(group_y, alice, bob, Some("requirement".to_string())).unwrap();

        let group_y_clone = group_y.clone();

        let bob_state = group_y_clone.members.get(&bob).unwrap();
        assert!(bob_state.is_writer());

        // Alice demotes Bob to Read access.
        let group_y = demote(group_y.clone(), alice, bob, None).unwrap();

        // Alice promotes Bob to Manage access.
        let group_y = modify(group_y, alice, bob, AccessLevel::Manage).unwrap();

        let bob_state = group_y.members.get(&bob).unwrap();
        assert!(bob_state.is_manager());
    }

    #[test]
    fn add_errors() {
        // "Unhappy path" test for add functions.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [(alice, AccessLevel::Manage), (bob, AccessLevel::Read)];

        // Alice creates a group with Alice and Bob as members.
        let group_y = create(&initial_members).unwrap();

        // Charlie adds Daphne...
        let result = add(group_y.clone(), charlie, daphne, AccessLevel::Read);

        // ...but Charlie isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor)
        ));

        // Bob adds Daphne...
        let result = add(group_y.clone(), bob, daphne, AccessLevel::Read);

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess)
        ));

        // Alice adds Bob...
        let result = add(group_y.clone(), alice, bob, AccessLevel::Read);

        // ...but Bob is already an active member.
        assert!(matches!(result, Err(GroupMembershipError::AlreadyAdded)));

        // Alice removes Bob.
        let group_y = remove(group_y, alice, bob).unwrap();

        // Bob adds Daphne...
        let result = add(group_y, bob, daphne, AccessLevel::Read);

        // ...but Bob isn't an active member.
        assert!(matches!(result, Err(GroupMembershipError::InactiveActor)));
    }

    #[test]
    fn remove_errors() {
        // "Unhappy path" test for remove functions.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Read),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members).unwrap();

        // Daphne removes Charlie...
        let result = remove(group_y.clone(), daphne, charlie);

        // ...but Daphne isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor)
        ));

        // Bob removes Charlie...
        let result = remove(group_y.clone(), bob, charlie);

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess)
        ));

        // Alice removes Daphne...
        let result = remove(group_y.clone(), alice, daphne);

        // ...but Daphne isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedMember)
        ));

        // Alice removes Charlie.
        let group_y = remove(group_y, alice, charlie).unwrap();

        // Alice removes Charlie...
        let result = remove(group_y, alice, charlie);

        // ...but Charlie has already been removed.
        assert!(matches!(result, Err(GroupMembershipError::AlreadyRemoved)));
    }

    #[test]
    fn promote_errors() {
        // "Unhappy path" test for promote functions.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Read),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members).unwrap();

        // Daphne promotes Charlie...
        let result = promote(group_y.clone(), daphne, charlie, None);

        // ...but Daphne isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor)
        ));

        // Bob promotes Charlie...
        let result = promote(group_y.clone(), bob, charlie, None);

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess)
        ));

        // Alice promotes Daphne...
        let result = promote(group_y.clone(), alice, daphne, None);

        // ...but Daphne isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedMember)
        ));

        // Alice removes Charlie.
        let group_y = remove(group_y, alice, charlie).unwrap();

        // Alice promotes Charlie...
        let result = promote(group_y.clone(), alice, charlie, None);

        // ...but Charlie isn't a member.
        assert!(matches!(result, Err(GroupMembershipError::InactiveMember)));

        // Charlie promotes Bob...
        let result = promote(group_y, charlie, bob, None);

        // ...but Charlie isn't a member.
        assert!(matches!(result, Err(GroupMembershipError::InactiveActor)));
    }

    #[test]
    fn demote_errors() {
        // "Unhappy path" test for demote functions.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Read),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members).unwrap();

        // Daphne demotes Charlie...
        let result = demote(group_y.clone(), daphne, charlie, None);

        // ...but Daphne isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor)
        ));

        // Bob demotes Charlie...
        let result = demote(group_y.clone(), bob, charlie, None);

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess)
        ));

        // Alice demotes Daphne...
        let result = demote(group_y.clone(), alice, daphne, None);

        // ...but Daphne isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedMember)
        ));

        // Alice removes Charlie.
        let group_y = remove(group_y, alice, charlie).unwrap();

        // Alice demotes Charlie...
        let result = demote(group_y.clone(), alice, charlie, None);

        // ...but Charlie isn't a member.
        assert!(matches!(result, Err(GroupMembershipError::InactiveMember)));

        // Charlie demotes Bob...
        let result = demote(group_y, charlie, bob, None);

        // ...but Charlie isn't a member.
        assert!(matches!(result, Err(GroupMembershipError::InactiveActor)));
    }

    #[test]
    fn merge_state_member() {
        // A member is added in one group state but not the other.
        // We expect the post-merge state to include the member.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Pull),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y_i = create(&initial_members).unwrap();

        // Alice adds Daphne.
        let group_y_ii = add(group_y_i.clone(), alice, daphne, AccessLevel::Read).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i, group_y_ii).unwrap();

        assert!(group_y.members().contains(&daphne));
    }

    #[test]
    fn merge_state_counter() {
        // A member exists in both group states but with different counters.
        // We expect the post-merge state to contain the higher of the two counters.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Pull),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y_i = create(&initial_members).unwrap();

        // Alice removes Bob.
        let group_y_ii = remove(group_y_i.clone(), alice, bob).unwrap();

        // Alice adds Bob.
        let group_y_ii = add(group_y_ii, alice, bob, AccessLevel::Read).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i, group_y_ii).unwrap();

        assert!(group_y.members().contains(&alice));
        assert!(group_y.members().contains(&bob));
        assert!(group_y.members().contains(&charlie));

        let bob_state = group_y.members.get(&bob).unwrap();

        // We expect the merge to choose the higher counter value for Bob.
        assert!(bob_state.member_counter == 3);
    }

    #[test]
    fn merge_state_access_counter() {
        // A member exists in both group states with equal counters but different access counters.
        // We expect the post-merge state to contain the higher of the two access counters.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Pull),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y_i = create(&initial_members).unwrap();

        // Alice promotes Charlie.
        let group_y_ii = promote(group_y_i.clone(), alice, charlie, None).unwrap();

        // Alice demotes Charlie.
        let group_y_ii = demote(group_y_ii.clone(), alice, charlie, None).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i, group_y_ii).unwrap();

        let charlie_state = group_y.members.get(&charlie).unwrap();

        // We expect the merge to choose the higher access counter value for Charlie.
        assert!(charlie_state.access_counter == 2);

        // We expect the access level to be Pull for Charlie.
        assert!(charlie_state.access == Access::Pull);
    }

    #[test]
    fn merge_state_access() {
        // A member exists in both group states with equal counters and equal access counters
        // but different access levels.
        // We expect the post-merge state to contain the lower of the two access levels.

        type AccessLevel = Access<String>;

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [
            (alice, AccessLevel::Manage),
            (bob, AccessLevel::Read),
            (charlie, AccessLevel::Pull),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members).unwrap();

        // Alice promotes Charlie.
        let group_y_i = promote(group_y.clone(), alice, charlie, None).unwrap();

        // Alice demotes Charlie.
        let group_y_i = demote(group_y_i.clone(), alice, charlie, None).unwrap();

        // Alice promotes Charlie.
        let group_y_ii = modify(group_y.clone(), alice, charlie, AccessLevel::Manage).unwrap();

        // Alice demotes Charlie.
        let group_y_ii = demote(
            group_y_ii.clone(),
            alice,
            charlie,
            Some("requirement".to_string()),
        )
        .unwrap();

        // Merge the states.
        let group_y = merge(group_y_i.clone(), group_y_ii.clone()).unwrap();

        let charlie_state = group_y.members.get(&charlie).unwrap();

        // We expect the access level to be Pull for Charlie.
        assert!(charlie_state.access == Access::Pull);
    }
}
