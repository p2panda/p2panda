// SPDX-License-Identifier: MIT OR Apache-2.0

//! Validation methods for group membership actions.

use std::{collections::HashSet, fmt::Debug};

use thiserror::Error;

use crate::AccessLevel;
use crate::group::{GroupCrdt, GroupCrdtInnerState, GroupCrdtState};
use crate::traits::{Conditions, IdentityHandle, Operation, OperationId, Resolver};

pub(crate) fn is_manager<ID>(actor: ID, members: &[(ID, AccessLevel)]) -> bool
where
    ID: Copy + Debug + PartialEq,
{
    let Some(actor) = members.iter().find(|(id, _)| *id == actor) else {
        return false;
    };

    if actor.1 != AccessLevel::Manage {
        return false;
    }

    true
}

pub(crate) fn is_writer<ID>(actor: ID, members: &[(ID, AccessLevel)]) -> bool
where
    ID: Copy + Debug + PartialEq,
{
    let Some(actor) = members.iter().find(|(id, _)| *id == actor) else {
        return false;
    };

    if actor.1 < AccessLevel::Write {
        return false;
    }

    true
}

pub(crate) fn members_at<ID, OP, M, C, RS>(
    y: &GroupCrdtState<ID, OP, M, C>,
    group_id: ID,
    heads: HashSet<OP>,
) -> Result<Vec<(ID, AccessLevel)>, MembersAtError>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    M: Operation<ID, OP, C> + Clone,
    C: Conditions,
    RS: Resolver<ID, OP, M, C, State = GroupCrdtInnerState<ID, OP, M, C>>,
{
    let members: Vec<_> = GroupCrdt::<ID, OP, M, C, RS>::members_at(y, heads, group_id)
        .map_err(|err| MembersAtError::RebuildFailure(err.to_string()))?
        .into_iter()
        .map(|(id, access)| (id, access.level))
        .collect();

    Ok(members)
}

pub(crate) fn is_member<ID>(member: ID, members: &[(ID, AccessLevel)]) -> bool
where
    ID: Copy + Debug + PartialEq,
{
    members.iter().any(|(id, _)| *id == member)
}

pub fn can_write<ID>(actor: ID, members: &[(ID, AccessLevel)]) -> Result<(), WriteError>
where
    ID: Copy + Debug + PartialEq,
{
    if !is_member(actor, members) {
        return Err(WriteError::UnrecognisedActor);
    }

    if !is_writer(actor, members) {
        return Err(WriteError::InsufficientAccess);
    }

    Ok(())
}

/// Check if the member has greater than or equal access to passed level.
pub(crate) fn has_gte_access<ID>(
    member: ID,
    access: AccessLevel,
    members: &[(ID, AccessLevel)],
) -> bool
where
    ID: Copy + Debug + PartialEq,
{
    members
        .iter()
        .any(|(id, inner_access)| *id == member && *inner_access >= access)
}

/// Check if the member has less than or equal access to passed level.
pub(crate) fn has_lte_access<ID>(
    member: ID,
    access: AccessLevel,
    members: &[(ID, AccessLevel)],
) -> bool
where
    ID: Copy + Debug + PartialEq,
{
    members
        .iter()
        .any(|(id, inner_access)| *id == member && *inner_access <= access)
}

/// Validate if a member can be added to a group.
///
/// The actor performing the action must be a member with manager rights and the to-be-added
/// member must not already be a member.
pub fn can_add_member<ID>(
    actor: ID,
    added: ID,
    members: &[(ID, AccessLevel)],
) -> Result<(), AddMemberError>
where
    ID: Copy + Debug + PartialEq,
{
    if !is_member(actor, members) {
        return Err(AddMemberError::UnrecognisedActor);
    };

    if !is_manager(actor, members) {
        return Err(AddMemberError::InsufficientAccess);
    }

    if is_member(added, members) {
        return Err(AddMemberError::AlreadyAdded);
    }

    Ok(())
}

/// Validate if a member can be removed from a group.
///
/// The actor performing the action must be a member with manager rights and the to-be-removed
/// member must be an existing member.
pub fn can_remove_member<ID>(
    actor: ID,
    removed: ID,
    members: &[(ID, AccessLevel)],
) -> Result<(), RemoveMemberError>
where
    ID: Copy + Debug + PartialEq,
{
    if !is_member(actor, members) {
        return Err(RemoveMemberError::UnrecognisedActor);
    };

    if !is_manager(actor, members) {
        return Err(RemoveMemberError::InsufficientAccess);
    }

    if !is_member(removed, members) {
        return Err(RemoveMemberError::NonMember);
    }

    Ok(())
}

/// Validate if an existing group member can be promoted to a higher access level.
///
/// The actor performing the action must be a member with manager rights and the to-be-promoted
/// member must already be a member with lower access level than the promotion gives.
pub fn can_promote_member<ID>(
    actor: ID,
    promoted: ID,
    access: AccessLevel,
    members: &[(ID, AccessLevel)],
) -> Result<(), PromoteMemberError>
where
    ID: Copy + Debug + PartialEq,
{
    if !is_member(actor, members) {
        return Err(PromoteMemberError::UnrecognisedActor);
    };

    if !is_manager(actor, members) {
        return Err(PromoteMemberError::InsufficientAccess);
    }

    if !is_member(promoted, members) {
        return Err(PromoteMemberError::NonMember);
    }

    if has_gte_access(promoted, access, members) {
        return Err(PromoteMemberError::HasGreaterOrEqualAccess);
    }

    Ok(())
}

/// Validate if an existing group member can be demoted to a lower access level.
///
/// The actor performing the action must be a member with manager rights and the to-be-demoted
/// member must already be a member with higher access level than the demotion allows.
pub fn can_demote_member<ID>(
    actor: ID,
    demoted: ID,
    access: AccessLevel,
    members: &[(ID, AccessLevel)],
) -> Result<(), DemoteMemberError>
where
    ID: Copy + Debug + PartialEq,
{
    if !is_member(actor, members) {
        return Err(DemoteMemberError::UnrecognisedActor);
    };

    if !is_manager(actor, members) {
        return Err(DemoteMemberError::InsufficientAccess);
    }

    if !is_member(demoted, members) {
        return Err(DemoteMemberError::NonMember);
    }

    if has_lte_access(demoted, access, members) {
        return Err(DemoteMemberError::HasLowerOrEqualAccess);
    }

    Ok(())
}

/// Verify that a member had write access at a specific point in the auth graphs history.
///
/// This is required when processing application messages which refer to a specific set of graph
/// heads. These can be considered as the authors claimed "proof" that they have write access. If
/// this validation passes it does not mean the author _still_ has write access. Their access
/// could have since, or concurrently, been removed.
///
/// Checks for these cases should be performed in further validation steps.  
pub fn verify_claimed_write_access<ID, OP, M, C, RS>(
    y: &GroupCrdtState<ID, OP, M, C>,
    actor: ID,
    group_id: ID,
    heads: HashSet<OP>,
) -> Result<(), VerifyClaimedWriteError>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    M: Operation<ID, OP, C> + Clone,
    C: Conditions,
    RS: Resolver<ID, OP, M, C, State = GroupCrdtInnerState<ID, OP, M, C>>,
{
    let members = members_at::<ID, OP, M, C, RS>(y, group_id, heads)?;
    can_write(actor, &members)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum VerifyClaimedWriteError {
    #[error("error computing members at claimed state: {0}")]
    MembersAt(#[from] MembersAtError),

    #[error("invalid claimed write access: {0}")]
    InvalidWrite(#[from] WriteError),
}

#[derive(Debug, Error)]
pub enum MembersAtError {
    #[error("error occurred rebuilding auth groups graph: {0}")]
    RebuildFailure(String),
}

#[derive(Debug, Error)]
pub enum WriteError {
    #[error("actor is not a member")]
    UnrecognisedActor,

    #[error("actor lacks sufficient access to perform a write action")]
    InsufficientAccess,
}

#[derive(Debug, Error)]
pub enum AddMemberError {
    #[error("actor is not a member")]
    UnrecognisedActor,

    #[error("actor lacks sufficient access to add a member")]
    InsufficientAccess,

    #[error("attempted to re-add an existing member")]
    AlreadyAdded,
}

#[derive(Debug, Error)]
pub enum RemoveMemberError {
    #[error("actor is not a member")]
    UnrecognisedActor,

    #[error("actor lacks sufficient access to remove a member")]
    InsufficientAccess,

    #[error("attempted to remove non-member actor")]
    NonMember,
}

#[derive(Debug, Error)]
pub enum PromoteMemberError {
    #[error("actor is not a member")]
    UnrecognisedActor,

    #[error("actor lacks sufficient access to add a member")]
    InsufficientAccess,

    #[error("member already holds greater or equal access")]
    HasGreaterOrEqualAccess,

    #[error("attempted to promote non-member actor")]
    NonMember,
}

#[derive(Debug, Error)]
pub enum DemoteMemberError {
    #[error("actor is not a member")]
    UnrecognisedActor,

    #[error("actor lacks sufficient access to add a member")]
    InsufficientAccess,

    #[error("member already holds lower or equal access")]
    HasLowerOrEqualAccess,

    #[error("attempted to demote non-member actor")]
    NonMember,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccessLevel;

    fn members() -> Vec<(u32, AccessLevel)> {
        vec![
            (1, AccessLevel::Manage),
            (2, AccessLevel::Write),
            (3, AccessLevel::Read),
        ]
    }

    #[test]
    fn validation() {
        // Is member validation
        assert!(is_member(1, &members()));
        assert!(is_member(2, &members()));
        assert!(is_member(3, &members()));
        assert!(!is_member(99, &members()));

        // Is manager validation
        assert!(is_manager(1, &members()));
        assert!(!is_manager(2, &members()));
        assert!(!is_manager(3, &members()));
        assert!(!is_manager(99, &members()));

        // Is writer validation
        assert!(is_writer(1, &members()));
        assert!(is_writer(2, &members()));
        assert!(!is_writer(3, &members()));
        assert!(!is_writer(99, &members()));

        // Can write errors
        assert!(can_write(1, &members()).is_ok());
        assert!(can_write(2, &members()).is_ok());
        let err = can_write(3, &members()).unwrap_err();
        assert!(matches!(err, WriteError::InsufficientAccess));
        let err = can_write(99, &members()).unwrap_err();
        assert!(matches!(err, WriteError::UnrecognisedActor));

        // Can add member errors
        assert!(can_add_member(1, 42, &members()).is_ok());
        let err = can_add_member(99, 42, &members()).unwrap_err();
        assert!(matches!(err, AddMemberError::UnrecognisedActor));
        let err = can_add_member(2, 42, &members()).unwrap_err();
        assert!(matches!(err, AddMemberError::InsufficientAccess));
        let err = can_add_member(3, 42, &members()).unwrap_err();
        assert!(matches!(err, AddMemberError::InsufficientAccess));
        let err = can_add_member(1, 2, &members()).unwrap_err();
        assert!(matches!(err, AddMemberError::AlreadyAdded));
        let err = can_add_member(99, 2, &members()).unwrap_err();
        assert!(matches!(err, AddMemberError::UnrecognisedActor));

        // Can remove member errors
        assert!(can_remove_member(1, 2, &members()).is_ok());
        assert!(can_remove_member(1, 1, &members()).is_ok());
        let err = can_remove_member(99, 2, &members()).unwrap_err();
        assert!(matches!(err, RemoveMemberError::UnrecognisedActor));
        let err = can_remove_member(2, 3, &members()).unwrap_err();
        assert!(matches!(err, RemoveMemberError::InsufficientAccess));
        let err = can_remove_member(1, 99, &members()).unwrap_err();
        assert!(matches!(err, RemoveMemberError::NonMember));

        // Can promote member errors
        assert!(can_promote_member(1, 2, AccessLevel::Manage, &members()).is_ok());
        let err = can_promote_member(99, 2, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, PromoteMemberError::UnrecognisedActor));
        let err = can_promote_member(2, 3, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, PromoteMemberError::InsufficientAccess));
        let err = can_promote_member(1, 99, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, PromoteMemberError::NonMember));
        let err = can_promote_member(1, 2, AccessLevel::Read, &members()).unwrap_err();
        assert!(matches!(err, PromoteMemberError::HasGreaterOrEqualAccess));
        let err = can_promote_member(1, 2, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, PromoteMemberError::HasGreaterOrEqualAccess));

        // Can demote member errors
        assert!(can_demote_member(1, 2, AccessLevel::Read, &members()).is_ok());
        let err = can_demote_member(99, 2, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, DemoteMemberError::UnrecognisedActor));
        let err = can_demote_member(2, 3, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, DemoteMemberError::InsufficientAccess));
        let err = can_demote_member(1, 99, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, DemoteMemberError::NonMember));
        let err = can_demote_member(1, 2, AccessLevel::Manage, &members()).unwrap_err();
        assert!(matches!(err, DemoteMemberError::HasLowerOrEqualAccess));
        let err = can_demote_member(1, 2, AccessLevel::Write, &members()).unwrap_err();
        assert!(matches!(err, DemoteMemberError::HasLowerOrEqualAccess));
    }
}
