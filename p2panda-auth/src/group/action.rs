// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Access;
use crate::group::GroupMember;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Actions for creating groups and modifying group membership.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum GroupAction<ID, C> {
    Create {
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    },
    Add {
        member: GroupMember<ID>,
        access: Access<C>,
    },
    Remove {
        member: GroupMember<ID>,
    },
    Promote {
        member: GroupMember<ID>,
        access: Access<C>,
    },
    Demote {
        member: GroupMember<ID>,
        access: Access<C>,
    },
}

impl<ID, C> GroupAction<ID, C>
where
    ID: Copy,
{
    /// Return `true` if this is a create action.
    pub fn is_create(&self) -> bool {
        matches!(self, GroupAction::Create { .. })
    }
}
