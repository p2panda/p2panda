// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::group::GroupAction;

/// Control messages which are processed in order to update group state.
///
/// There are two variants, one containing a group action and the ID of the group to which the
/// action should be applied. The other is a special message which can be used to "undo" a message which
/// has been previously applied to the group.
#[derive(Clone, Debug)]
pub enum GroupControlMessage<ID, OP, C> {
    /// An action to apply to the state of the specified group.
    GroupAction {
        group_id: ID,
        action: GroupAction<ID, C>,
    },

    /// A revocation of the given operation for the specified group.
    ///
    /// Publishing a revocation message allows for invalidation of other group control messages
    /// which are already included in the group graph. This action is agnostic to any, probably more
    /// nuanced, resolving logic which reacts to group actions.
    //
    // TODO: revoking messages is not implemented yet. I'm still considering if it is required in
    // or initial group implementation or something that can come later. There are distinct
    // benefits to revoking messages, over "just" making sure to resolve concurrent group action
    // conflicts (for example with "strong removal") strategy. By issuing a revoke message
    // revoking the message which first added a member into the group, it's possible to
    // completely erase that member from the group history. There can be an implicit "seniority"
    // rule in play, where it's only possible for an admin to revoke messages that they
    // published, or from when they were in the group.
    Revoke { group_id: ID, id: OP },
}

impl<ID, OP, C> GroupControlMessage<ID, OP, C>
where
    ID: Copy,
{
    /// Return `true` if this is a create control message.
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            GroupControlMessage::GroupAction {
                action: GroupAction::Create { .. },
                ..
            }
        )
    }

    /// Return the ID of the group this message should be applied to.
    pub fn group_id(&self) -> ID {
        match self {
            GroupControlMessage::Revoke { group_id, .. } => *group_id,
            GroupControlMessage::GroupAction { group_id, .. } => *group_id,
        }
    }
}
