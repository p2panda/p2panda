// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::traits::Operation as AuthOperation;

use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::types::{ActorId, AuthControlMessage, Conditions, OperationId};

#[derive(Clone, Debug)]
pub struct AuthArgs<C> {
    // @TODO: Here we will fill in the "dependencies", control message etc. which will be later
    // used by ForgeArgs.
    pub(crate) control_message: AuthControlMessage<C>,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AuthMessage<C> {
    Args(AuthArgs<C>),
    Forged {
        author: ActorId,
        operation_id: OperationId,
        control_message: AuthControlMessage<C>,
    },
}

impl<C> AuthMessage<C>
where
    C: Conditions,
{
    pub(crate) fn from_forged<M>(message: &M) -> Self
    where
        M: AuthoredMessage + SpacesMessage<C>,
    {
        AuthMessage::Forged {
            author: message.author(),
            operation_id: message.id(),
            control_message: match message.args() {
                SpacesArgs::ControlMessage {
                    id,
                    control_message,
                    ..
                } => AuthControlMessage {
                    group_id: *id,
                    action: control_message.to_auth_action(),
                },
                _ => panic!("unexpected message type"),
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
        // @TODO: We do not implement ordering yet.
        Vec::new()
    }

    fn previous(&self) -> Vec<OperationId> {
        // @TODO: We do not implement ordering yet.
        Vec::new()
    }

    fn payload(&self) -> AuthControlMessage<C> {
        match self {
            AuthMessage::Args(_) => {
                // Nothing of this will ever be called at this stage where we're just preparing the
                // arguments for a future message to be forged.
                unreachable!()
            }
            AuthMessage::Forged {
                control_message, ..
            } => control_message.clone(),
        }
    }
}
