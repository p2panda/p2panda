use p2panda_core::{Hash, PublicKey};

use crate::group::GroupAction;
use crate::traits::{Conditions, IdentityHandle, Operation, OperationId};

/// Concrete groups control message type.
#[derive(Clone, Debug)]
pub struct GroupsOperation<A = PublicKey, ID = Hash, C = ()> {
    pub(crate) id: ID,
    pub(crate) author: A,
    pub(crate) dependencies: Vec<ID>,
    pub(crate) group_id: A,
    pub(crate) action: GroupAction<A, C>,
}

/// Implementation of groups Operation trait.
impl<A, ID, C> Operation<A, ID, C> for GroupsOperation<A, ID, C>
where
    A: IdentityHandle,
    ID: OperationId,
    C: Conditions,
{
    fn id(&self) -> ID {
        self.id
    }

    fn author(&self) -> A {
        self.author
    }

    fn dependencies(&self) -> Vec<ID> {
        self.dependencies.clone()
    }

    fn group_id(&self) -> A {
        self.group_id
    }

    fn action(&self) -> GroupAction<A, C> {
        self.action.clone()
    }
}
