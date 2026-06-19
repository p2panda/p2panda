// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};
use serde::{Deserialize, Serialize};

use crate::{ActorId, GroupId, OperationId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthMessage<C> {
    pub(crate) operation_id: OperationId,
    pub(crate) author: ActorId,
    pub(crate) dependencies: Vec<OperationId>,
    pub(crate) group_id: GroupId,
    pub(crate) action: GroupAction<ActorId, C>,
}

impl<C> AuthOperation<ActorId, OperationId, C> for AuthMessage<C>
where
    C: Conditions,
{
    fn id(&self) -> OperationId {
        self.operation_id.to_owned()
    }

    fn author(&self) -> ActorId {
        self.author.to_owned()
    }

    fn dependencies(&self) -> Vec<OperationId> {
        self.dependencies.to_owned()
    }

    fn group_id(&self) -> GroupId {
        self.group_id.to_owned()
    }

    fn action(&self) -> p2panda_auth::group::GroupAction<ActorId, C> {
        self.action.to_owned()
    }
}
