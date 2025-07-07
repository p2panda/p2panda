// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core group membership state represented as a Causal Length CRDT (CL-CRDT).
//!
//! The approach used here was first described by Weihai Yu and Sigbjørn Rostad and in their paper
//! titled 'A low-cost set CRDT based on causal lengths'.
//!
//! Yu, W. and Rostad, S. A Low-Cost Set CRDT Based on Causal Lengths. In Proceedings of the 7th
//! Workshop on Principles and Practice of Consistency for Distributed Data (2020), Article no. 5,
//! pp. 1-6.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use thiserror::Error;

use crate::Access;

/// Invalid group state modification attempts due to group membership state and member access
/// levels.
#[derive(Debug, Error, PartialEq)]
pub enum GroupMembershipError<ID> {
    #[error("attempted to add a member who is already active in the group: {0}")]
    AlreadyAdded(ID),

    #[error("attempted to remove a member who is already inactive in the group: {0}")]
    AlreadyRemoved(ID),

    #[error("actor lacks sufficient access to update the group: {0}")]
    InsufficientAccess(ID),

    #[error("actor is not an active member of the group: {0}")]
    InactiveActor(ID),

    #[error("member is not an active member of the group: {0}")]
    InactiveMember(ID),

    #[error("actor is not known to the group: {0}")]
    UnrecognisedActor(ID),

    #[error("member is not known to the group: {0}")]
    UnrecognisedMember(ID),
}

/// The access state of an individual group member.
///
/// Counters are used to allow conflict-free merging of states.
#[derive(Clone, Debug)]
pub struct MemberState<C> {
    pub(crate) member_counter: usize,
    pub(crate) access: Access<C>,
    pub(crate) access_counter: usize,
}

impl<C> MemberState<C>
where
    C: Clone + Debug + PartialEq,
{
    /// Return the access level of the member.
    pub fn access(&self) -> Access<C> {
        self.access.clone()
    }

    /// Return `true` if the member is an active member of the group.
    pub fn is_member(&self) -> bool {
        self.member_counter % 2 != 0
    }

    /// Return `true` if the member has `Pull` access.
    pub fn is_puller(&self) -> bool {
        self.access.is_pull()
    }

    /// Return `true` if the member has `Read` access.
    pub fn is_reader(&self) -> bool {
        self.access.is_read()
    }

    /// Return `true` if the member has `Write` access.
    pub fn is_writer(&self) -> bool {
        self.access.is_write()
    }

    /// Return `true` if the member has `Manage` access.
    pub fn is_manager(&self) -> bool {
        self.access.is_manage()
    }
}

/// The membership state of all known groups.
#[derive(Clone, Debug)]
pub struct GroupMembersState<ID, C> {
    pub(crate) members: HashMap<ID, MemberState<C>>,
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

    /// Return all active group members with `Manage` access.
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

/// Create a new group and add the given set of initial members.
pub fn create<ID: Clone + Eq + Hash, C: Clone + PartialEq>(
    initial_members: &[(ID, Access<C>)],
) -> GroupMembersState<ID, C> {
    let mut members = HashMap::new();
    for (id, access) in initial_members {
        let member = MemberState {
            member_counter: 1,
            access: access.clone(),
            access_counter: 0,
        };
        members.insert(id.clone(), member);
    }

    GroupMembersState { members }
}

/// Add a member to the group with the given access level.
///
/// The `adder` must be an active member of the group with `Manage` access and the `added` identity
/// must not be a current member of the group; failure to meet these conditions will result in an
/// error.
///
/// Re-adding a previously removed member is supported.
pub fn add<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq>(
    state: GroupMembersState<ID, C>,
    adder: ID,
    added: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError<ID>> {
    // Ensure that "adder" is known to the group.
    let Some(adder_state) = state.members.get(&adder) else {
        return Err(GroupMembershipError::UnrecognisedActor(adder));
    };

    // Ensure that "adder" is a member of the group with manage access level.
    if !adder_state.is_member() {
        return Err(GroupMembershipError::InactiveActor(adder));
    } else if !adder_state.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess(adder));
    }

    // Ensure that "added" is not already an active member of the group.
    if let Some(added_state) = state.members.get(&added) {
        if added_state.is_member() {
            return Err(GroupMembershipError::AlreadyAdded(added));
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
/// The `remover` must be an active member of the group with `Manage` access and the `removed`
/// identity must also be an active member of the group; failure to meet these conditions will
/// result in an error.
pub fn remove<ID: Eq + Hash, C: Clone + Debug + PartialEq>(
    state: GroupMembersState<ID, C>,
    remover: ID,
    removed: ID,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError<ID>> {
    // Ensure that "remover" is known to the group.
    let Some(remover_state) = state.members.get(&remover) else {
        return Err(GroupMembershipError::UnrecognisedActor(remover));
    };

    // Ensure that "remover" is a member of the group with manage access level.
    if !remover_state.is_member() {
        return Err(GroupMembershipError::InactiveActor(remover));
    } else if !remover_state.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess(remover));
    }

    // Ensure that "removed" is known to the group.
    if !state.members.contains_key(&removed) {
        return Err(GroupMembershipError::UnrecognisedMember(removed));
    };

    // Ensure that "removed" is not already an inactive member of the group.
    if let Some(removed_state) = state.members.get(&removed) {
        if !removed_state.is_member() {
            return Err(GroupMembershipError::AlreadyRemoved(removed));
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
/// Both the `modifier` and `modified` identity must be active group members; failure to meet these
/// conditions will result in an error.
///
/// This is a helper method to reduce code duplication in `promote()` and `demote()`.
fn modify<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    modifier: ID,
    modified: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError<ID>> {
    // Ensure that "modifier" is known to the group.
    let Some(modifier_state) = state.members.get(&modifier) else {
        return Err(GroupMembershipError::UnrecognisedActor(modifier));
    };

    // Ensure that "modifier" is a member of the group with manage access level.
    if !modifier_state.is_member() {
        return Err(GroupMembershipError::InactiveActor(modifier));
    } else if !modifier_state.is_manager() {
        return Err(GroupMembershipError::InsufficientAccess(modifier));
    }

    // Ensure that "modified" is an active member of the group.
    if let Some(modified_state) = state.members.get(&modified) {
        if !modified_state.is_member() {
            return Err(GroupMembershipError::InactiveMember(modified));
        }
    } else {
        return Err(GroupMembershipError::UnrecognisedMember(modified));
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

/// Promote a group member to the given access level.
///
/// No modification will occur if the promoted member already has `Manage` access. In that case, the
/// given state is returned unchanged.
///
/// The `promoter` must be an active member of the group with `Manage` access and the `promoted`
/// identity must also be an active member of the group; failure to meet these conditions will
/// result in an error.
pub fn promote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    promoter: ID,
    promoted: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError<ID>> {
    if let Some(member) = state.members.get(&promoted) {
        // No action is required if the member is already set to the highest access level.
        let new_state = if member.is_manager() {
            state
        } else {
            modify(state, promoter, promoted, access)?
        };

        Ok(new_state)
    } else {
        Err(GroupMembershipError::UnrecognisedMember(promoted))
    }
}

/// Demote a group member to the given access level.
///
/// No modification will occur if the demoted member already has `Pull` access. In that case, the
/// given state is returned unchanged.
///
/// The `demoter` must be an active member of the group with `Manage` access and the `demoted`
/// identity must also be an active member of the group; failure to meet these conditions will
/// result in an error.
pub fn demote<ID: Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state: GroupMembersState<ID, C>,
    demoter: ID,
    demoted: ID,
    access: Access<C>,
) -> Result<GroupMembersState<ID, C>, GroupMembershipError<ID>> {
    if let Some(member) = state.members.get(&demoted) {
        // No action is required if the member is already set to the lowest access level.
        let new_state = if member.is_puller() {
            state
        } else {
            modify(state, demoter, demoted, access)?
        };

        Ok(new_state)
    } else {
        Err(GroupMembershipError::UnrecognisedMember(demoted))
    }
}

/// Merge two group states into one using a deterministic, conflict-free approach.
///
/// Grow-only counters are used internally to track state changes; one counter for add / remove
/// actions and one for access modification actions. These values are used to determine which
/// membership and access states should be included in the merged group state. A state with a higher
/// counter indicates that it has undergone more actions; this state will be included in the merge.
///
/// If a member exists with different access levels in each state but the same number of access
/// modifications, the lower of the two access levels will be chosen.
pub fn merge<ID: Clone + Eq + Hash, C: Clone + Debug + PartialEq + PartialOrd>(
    state_1: GroupMembersState<ID, C>,
    state_2: GroupMembersState<ID, C>,
) -> GroupMembersState<ID, C> {
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

    next_state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_add_remove() {
        // "Happy path" test for create, add and remove functions.

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [(alice, Access::manage()), (bob, Access::read())];

        // Alice creates a group with Alice and Bob as members.
        let group_y = create(&initial_members);

        assert!(group_y.members().contains(&alice));
        assert!(group_y.members().contains(&bob));

        assert!(group_y.managers().contains(&alice));
        assert!(!group_y.managers().contains(&bob));

        // Alice adds Charlie.
        let group_y = add(
            group_y,
            alice,
            charlie,
            Access::write().with_conditions("requirement".to_string()),
        )
        .unwrap();

        assert!(group_y.members().contains(&charlie));

        // Alice removes Bob.
        let group_y = remove(group_y, alice, bob).unwrap();

        assert!(!group_y.members().contains(&bob));
    }

    #[test]
    fn promote_demote_modify() {
        let alice = 0;
        let bob = 1;

        let initial_members = [(alice, Access::manage()), (bob, Access::read())];

        // Alice creates a group with Alice and Bob as members.
        let group_y = create(&initial_members);

        // Alice promotes Bob to Write access.
        let group_y = promote(
            group_y,
            alice,
            bob,
            Access::write().with_conditions("requirement".to_string()),
        )
        .unwrap();

        let group_y_clone = group_y.clone();

        let bob_state = group_y_clone.members.get(&bob).unwrap();
        assert!(bob_state.is_writer());

        // Alice demotes Bob to Read access.
        let group_y = demote(group_y.clone(), alice, bob, Access::read()).unwrap();

        // Alice promotes Bob to Manage access.
        let group_y = modify(group_y, alice, bob, Access::manage()).unwrap();

        let bob_state = group_y.members.get(&bob).unwrap();
        assert!(bob_state.is_manager());
    }

    #[test]
    fn add_errors() {
        // "Unhappy path" test for add functions.

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [(alice, <Access>::manage()), (bob, Access::read())];

        // Alice creates a group with Alice and Bob as members.
        let group_y = create(&initial_members);

        // Charlie adds Daphne...
        let result = add(group_y.clone(), charlie, daphne, Access::read());

        // ...but Charlie isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor(_bob))
        ));

        // Bob adds Daphne...
        let result = add(group_y.clone(), bob, daphne, Access::read());

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess(_bob))
        ));

        // Alice adds Bob...
        let result = add(group_y.clone(), alice, bob, Access::read());

        // ...but Bob is already an active member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::AlreadyAdded(_bob))
        ));

        // Alice removes Bob.
        let group_y = remove(group_y, alice, bob).unwrap();

        // Bob adds Daphne...
        let result = add(group_y, bob, daphne, Access::read());

        // ...but Bob isn't an active member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InactiveActor(_bob))
        ));

        // TODO.
        // The `assert!(matches!())` tests don't test the value in the variant tuple.
        // We should consider rather using `if let` to match fully.
        /*
        if let Err(GroupMembershipError::InactiveActor(actor)) = result {
            assert_eq!(actor, bob)
        } else {
            panic!("description goes here...")
        }
        */
    }

    #[test]
    fn remove_errors() {
        // "Unhappy path" test for remove functions.

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, <Access>::manage()),
            (bob, Access::read()),
            (charlie, Access::read()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members);

        // Daphne removes Charlie...
        let result = remove(group_y.clone(), daphne, charlie);

        // ...but Daphne isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor(_daphne))
        ));

        // Bob removes Charlie...
        let result = remove(group_y.clone(), bob, charlie);

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess(_bob))
        ));

        // Alice removes Daphne...
        let result = remove(group_y.clone(), alice, daphne);

        // ...but Daphne isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedMember(_daphne))
        ));

        // Alice removes Charlie.
        let group_y = remove(group_y, alice, charlie).unwrap();

        // Alice removes Charlie...
        let result = remove(group_y, alice, charlie);

        // ...but Charlie has already been removed.
        assert!(matches!(
            result,
            Err(GroupMembershipError::AlreadyRemoved(_charlie))
        ));
    }

    #[test]
    fn promote_errors() {
        // "Unhappy path" test for promote functions.

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, Access::manage()),
            (bob, Access::read()),
            (charlie, Access::read()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members);

        // Daphne promotes Charlie...
        let result = promote(group_y.clone(), daphne, charlie, Access::manage());

        // ...but Daphne isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor(_daphne))
        ));

        // Bob promotes Charlie...
        let result = promote(
            group_y.clone(),
            bob,
            charlie,
            Access::write().with_conditions("paw".to_string()),
        );

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess(_bob))
        ));

        // Alice promotes Daphne...
        let result = promote(group_y.clone(), alice, daphne, Access::read());

        // ...but Daphne isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedMember(_daphne))
        ));

        // Alice removes Charlie.
        let group_y = remove(group_y, alice, charlie).unwrap();

        // Alice promotes Charlie...
        let result = promote(group_y.clone(), alice, charlie, Access::pull());

        // ...but Charlie isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InactiveMember(_charlie))
        ));

        // Charlie promotes Bob...
        let result = promote(group_y, charlie, bob, Access::manage());

        // ...but Charlie isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InactiveActor(_charlie))
        ));
    }

    #[test]
    fn demote_errors() {
        // "Unhappy path" test for demote functions.

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, <Access>::manage()),
            (bob, Access::read()),
            (charlie, Access::read()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members);

        // Daphne demotes Charlie...
        let result = demote(group_y.clone(), daphne, charlie, Access::pull());

        // ...but Daphne isn't known to the group (has never been a member).
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedActor(_daphne))
        ));

        // Bob demotes Charlie...
        let result = demote(group_y.clone(), bob, charlie, Access::pull());

        // ...but Bob isn't a manager.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InsufficientAccess(_bob))
        ));

        // Alice demotes Daphne...
        let result = demote(group_y.clone(), alice, daphne, Access::read());

        // ...but Daphne isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::UnrecognisedMember(_daphne))
        ));

        // Alice removes Charlie.
        let group_y = remove(group_y, alice, charlie).unwrap();

        // Alice demotes Charlie...
        let result = demote(group_y.clone(), alice, charlie, Access::pull());

        // ...but Charlie isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InactiveMember(_charlie))
        ));

        // Charlie demotes Bob...
        let result = demote(group_y, charlie, bob, Access::pull());

        // ...but Charlie isn't a member.
        assert!(matches!(
            result,
            Err(GroupMembershipError::InactiveActor(_charlie))
        ));
    }

    #[test]
    fn merge_state_member() {
        // A member is added in one group state but not the other.
        // We expect the post-merge state to include the member.

        let alice = 0;
        let bob = 1;
        let charlie = 2;
        let daphne = 3;

        let initial_members = [
            (alice, <Access>::manage()),
            (bob, Access::read()),
            (charlie, Access::pull()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y_i = create(&initial_members);

        // Alice adds Daphne.
        let group_y_ii = add(group_y_i.clone(), alice, daphne, Access::read()).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i, group_y_ii);

        assert!(group_y.members().contains(&daphne));
    }

    #[test]
    fn merge_state_counter() {
        // A member exists in both group states but with different counters.
        // We expect the post-merge state to contain the higher of the two counters.

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [
            (alice, <Access>::manage()),
            (bob, Access::read()),
            (charlie, Access::pull()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y_i = create(&initial_members);

        // Alice removes Bob.
        let group_y_ii = remove(group_y_i.clone(), alice, bob).unwrap();

        // Alice adds Bob.
        let group_y_ii = add(group_y_ii, alice, bob, Access::read()).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i, group_y_ii);

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

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [
            (alice, <Access>::manage()),
            (bob, Access::read()),
            (charlie, Access::pull()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y_i = create(&initial_members);

        // Alice promotes Charlie.
        let group_y_ii = promote(group_y_i.clone(), alice, charlie, Access::read()).unwrap();

        // Alice demotes Charlie.
        let group_y_ii = demote(group_y_ii.clone(), alice, charlie, Access::pull()).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i, group_y_ii);

        let charlie_state = group_y.members.get(&charlie).unwrap();

        // We expect the merge to choose the higher access counter value for Charlie.
        assert!(charlie_state.access_counter == 2);

        // We expect the access level to be Pull for Charlie.
        assert!(charlie_state.is_puller());
    }

    #[test]
    fn merge_state_access() {
        // A member exists in both group states with equal counters and equal access counters
        // but different access levels.
        // We expect the post-merge state to contain the lower of the two access levels.

        let alice = 0;
        let bob = 1;
        let charlie = 2;

        let initial_members = [
            (alice, <Access>::manage()),
            (bob, Access::read()),
            (charlie, Access::pull()),
        ];

        // Alice creates a group with Alice, Bob and Charlie as members.
        let group_y = create(&initial_members);

        // Alice promotes Charlie.
        let group_y_i = promote(group_y.clone(), alice, charlie, Access::read()).unwrap();

        // Alice demotes Charlie.
        let group_y_i = demote(group_y_i.clone(), alice, charlie, Access::pull()).unwrap();

        // Alice promotes Charlie.
        let group_y_ii = modify(group_y.clone(), alice, charlie, Access::manage()).unwrap();

        // Alice demotes Charlie.
        let group_y_ii = demote(group_y_ii.clone(), alice, charlie, Access::read()).unwrap();

        // Merge the states.
        let group_y = merge(group_y_i.clone(), group_y_ii.clone());

        let charlie_state = group_y.members.get(&charlie).unwrap();

        // We expect the access level to be Pull for Charlie.
        assert!(charlie_state.is_puller());
    }
}
