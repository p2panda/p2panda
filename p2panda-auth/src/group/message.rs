// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::group::GroupAction;

/// Control messages which are processed by a group.
///
/// There are two variants, one containing a group action and the id of the group where the action
/// should be applied. The other is a special message which can be used to "undo" a message which
/// has been applied to the group in the past.
#[derive(Clone, Debug)]
pub enum GroupControlMessage<ID, OP, C> {
    /// An action to apply to the group state.
    GroupAction {
        group_id: ID,
        action: GroupAction<ID, C>,
    },

    /// A revoke message can be published in order to explicitly invalidate other messages already
    /// included in a group graph. This action is agnostic to any, probably more nuanced,
    /// resolving logic which reacts to group actions.
    ///
    /// TODO: revoking messages is not implemented yet. I'm still considering if it is required in
    /// or initial group implementation or something that can come later. There are distinct
    /// benefits to revoking messages, over "just" making sure to resolve concurrent group action
    /// conflicts (for example with "strong removal") strategy. By issuing a revoke message
    /// revoking the message which first added a member into the group, it's possible to
    /// completely erase that member from the group history. There can be an implicit "seniority"
    /// rule in play, where it's only possible for an admin to revoke messages that they
    /// published, or from when they were in the group.
    Revoke { group_id: ID, id: OP },
}

impl<ID, OP, C> GroupControlMessage<ID, OP, C>
where
    ID: Copy,
{
    /// Returns true if this is a create control message.
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            GroupControlMessage::GroupAction {
                action: GroupAction::Create { .. },
                ..
            }
        )
    }

    /// Id of the group this message should be applied to.
    pub fn group_id(&self) -> ID {
        match self {
            GroupControlMessage::Revoke { group_id, .. } => *group_id,
            GroupControlMessage::GroupAction { group_id, .. } => *group_id,
        }
    }
}
