use std::collections::HashMap;
use std::hash::Hash;

use petgraph::prelude::DiGraphMap;

use crate::group::access::Access;
use crate::group::state::GroupMembersState;
use crate::traits::Ordering;

mod access;
mod resolver;
mod state;

/// Actions which can be performed by group members.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupAction<ID> {
    Create { initial_members: Vec<(ID, Access)> },
    Add { member: ID, access: Access },
    Remove { member: ID },
    Promote { member: ID, access: Access },
    Demote { member: ID, access: Access },
}

impl<ID> GroupAction<ID> {
    pub fn is_create(&self) -> bool {
        if let GroupAction::Create { .. } = self {
            true
        } else {
            false
        }
    }
}

/// Control messages which are processed by a group.
#[derive(Clone, Debug)]
pub enum GroupControlMessage<ID, OP> {
    Revoke { id: OP },
    GroupAction(GroupAction<ID>),
}

/// The internal state of a group.
#[derive(Clone, Debug)]
pub struct GroupState<ID, OP, ORD>
where
    OP: Copy + Eq + Hash,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    // ID of the local actor.
    pub my_id: ID,

    /// Operation graph.
    pub graph: DiGraphMap<OP, ()>,

    /// States at every position in the message graph.
    pub states: HashMap<OP, GroupMembersState<ID>>,

    /// All operations processed by this group.
    pub operations: HashMap<OP, ORD::Message>,
}
