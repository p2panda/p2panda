// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::traits::IdentityHandle;

/// A group member which can be a single individual or another group.
///
/// The `Group` variant can be used to express nested group relations. In both cases, the member
/// identifier is the same generic ID.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum GroupMember<ID> {
    Individual(ID),
    Group(ID),
}

impl<ID> GroupMember<ID>
where
    ID: Copy,
{
    /// Return the ID of a group member.
    pub fn id(&self) -> ID {
        match self {
            GroupMember::Individual(id) => *id,
            GroupMember::Group(id) => *id,
        }
    }

    /// Return true if this group member is itself a group.
    pub fn is_group(&self) -> bool {
        match self {
            GroupMember::Individual(_) => false,
            GroupMember::Group(_) => true,
        }
    }

    /// Return true if this group member is an individual.
    pub fn is_individual(&self) -> bool {
        !self.is_group()
    }
}

impl<ID> IdentityHandle for GroupMember<ID> where ID: IdentityHandle {}
