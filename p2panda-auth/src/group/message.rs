// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(any(test, feature = "serde"))]
use serde::{Deserialize, Serialize};

use crate::group::GroupAction;

/// Control messages which are processed in order to update group state.
///
/// There are two variants, one containing a group action and the ID of the group to which the
/// action should be applied. The other is a special message which can be used to "undo" a message which
/// has been previously applied to the group.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(any(test, feature = "serde"), derive(Deserialize, Serialize))]
pub struct GroupControlMessage<ID, C> {
    pub group_id: ID,
    pub action: GroupAction<ID, C>,
}

impl<ID, C> GroupControlMessage<ID, C>
where
    ID: Copy,
{
    /// Return `true` if this is a create control message.
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            GroupControlMessage {
                action: GroupAction::Create { .. },
                ..
            }
        )
    }

    /// Return the ID of the group this message should be applied to.
    pub fn group_id(&self) -> ID {
        self.group_id
    }
}
