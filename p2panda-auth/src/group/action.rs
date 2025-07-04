// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{group::GroupMember};
use crate::Access;

/// Actions which can be applied to a group.
#[derive(Clone, Debug, PartialEq)]
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
    /// Returns true if this is a create action.
    pub fn is_create(&self) -> bool {
        matches!(self, GroupAction::Create { .. })
    }
}