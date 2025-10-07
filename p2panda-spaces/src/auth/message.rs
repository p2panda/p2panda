// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::traits::{Conditions, Operation as AuthOperation};

use crate::message::SpacesArgs;
use crate::traits::SpaceId;
use crate::traits::message::{AuthoredMessage, SpacesMessage};
use crate::types::{ActorId, AuthControlMessage, OperationId};

#[derive(Clone, Debug)]
pub struct AuthArgs<C> {
    pub(crate) dependencies: Vec<OperationId>,
    pub(crate) control_message: AuthControlMessage<C>,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AuthMessage<C> {
    Args(AuthArgs<C>),
    Forged {
        author: ActorId,
        operation_id: OperationId,
        args: AuthArgs<C>,
    },
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
            control_message,
            auth_dependencies,
        } = message.args()
        else {
            panic!("unexpected message type")
        };
        AuthMessage::Forged {
            author: message.author(),
            operation_id: message.id(),
            args: AuthArgs {
                dependencies: auth_dependencies.clone(),
                control_message: control_message.to_owned(),
            },
        }
    }
}

impl<C> AuthOperation<ActorId, OperationId, AuthControlMessage<C>> for AuthMessage<C>
where
    C: Conditions,
{
    fn id(&self) -> OperationId {
        match self {
            AuthMessage::Args(_) => {
                // Nothing of this will ever be called at this stage where we're just preparing the
                // arguments for a future message to be forged.
                unreachable!()
            }
            AuthMessage::Forged { operation_id, .. } => *operation_id,
        }
    }

    fn author(&self) -> ActorId {
        match self {
            AuthMessage::Args(_) => {
                // Nothing of this will ever be called at this stage where we're just preparing the
                // arguments for a future message to be forged.
                unreachable!()
            }
            AuthMessage::Forged { author, .. } => *author,
        }
    }

    fn dependencies(&self) -> Vec<OperationId> {
        match self {
            AuthMessage::Args(args) => args.dependencies.clone(),
            AuthMessage::Forged {
                args: AuthArgs { dependencies, .. },
                ..
            } => dependencies.clone(),
        }
    }

    fn payload(&self) -> AuthControlMessage<C> {
        match self {
            AuthMessage::Args(args) => args.control_message.clone(),
            AuthMessage::Forged {
                args: AuthArgs {
                    control_message, ..
                },
                ..
            } => control_message.clone(),
        }
    }
}
