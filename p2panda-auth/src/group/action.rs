// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(any(test, feature = "serde"))]
use serde::{Deserialize, Serialize};

use crate::Access;
use crate::group::GroupMember;

/// Actions for creating groups and modifying group membership.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "serde"), derive(Deserialize, Serialize))]
pub enum GroupAction<ID, C = ()> {
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

    /// Returns groups which are dependant on being able to process this action.
    ///
    /// This calculation looks into the members being added / removed by the action and returns
    /// only the members which are groups.
    pub fn required_groups(&self) -> Vec<ID> {
        let members = match self {
            GroupAction::Create { initial_members } => {
                initial_members.iter().map(|(member, _)| member).collect()
            }
            GroupAction::Add { member, .. }
            | GroupAction::Remove { member }
            | GroupAction::Promote { member, .. }
            | GroupAction::Demote { member, .. } => vec![member],
        };
        members
            .iter()
            .filter_map(|member| match member {
                crate::group::GroupMember::Individual(_) => None,
                crate::group::GroupMember::Group(id) => Some(*id),
            })
            .collect::<Vec<_>>()
    }
}
