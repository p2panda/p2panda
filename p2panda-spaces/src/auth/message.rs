// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};

use crate::message::SpacesArgs;
use crate::traits::{AuthoredMessage, SpaceId, SpacesMessage};
use crate::types::{ActorId, OperationId};

#[derive(Clone, Debug)]
pub struct AuthMessage<C> {
    operation_id: OperationId,
    author: ActorId,
    dependencies: Vec<OperationId>,
    group_id: ActorId,
    action: GroupAction<ActorId, C>,
}

impl<C> AuthMessage<C>
where
    C: Conditions,
{
    pub(crate) fn from_forged<ID, M>(message: &M) -> Self
    where
        ID: SpaceId,
        M: AuthoredMessage + SpacesMessage<ID, C>,
    {
        let SpacesArgs::Auth {
            group_id,
            group_action,
            auth_dependencies,
        } = message.args()
        else {
            panic!("unexpected message type")
        };
        AuthMessage {
            operation_id: message.id(),
            author: message.author(),
            dependencies: auth_dependencies.to_owned(),
            group_id: *group_id,
            action: group_action.to_owned(),
        }
    }
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

    fn group_id(&self) -> ActorId {
        self.group_id.to_owned()
    }

    fn action(&self) -> p2panda_auth::group::GroupAction<ActorId, C> {
        self.action.to_owned()
    }
}
