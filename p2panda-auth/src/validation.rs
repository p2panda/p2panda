// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use thiserror::Error;

use crate::AccessLevel;

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
        return Err(RemoveMemberError::NonMemberRemove);
    }

    Ok(())
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
    NonMemberRemove,
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
        assert!(matches!(err, RemoveMemberError::NonMemberRemove));
    }
}
