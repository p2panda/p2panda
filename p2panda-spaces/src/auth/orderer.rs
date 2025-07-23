// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_auth::traits::Operation as AuthOperation;

use crate::forge::ForgedMessage;
use crate::types::{ActorId, AuthControlMessage, Conditions, OperationId};

// Manages "dependencies" required for `p2panda-auth`.
#[derive(Clone, Debug)]
pub struct AuthOrderer {}

impl AuthOrderer {
    pub fn new() -> Self {
        Self {}
    }
}

impl<C> p2panda_auth::traits::Orderer<ActorId, OperationId, AuthControlMessage<C>> for AuthOrderer
where
    C: Conditions,
{
    type State = (); // @TODO

    type Operation = AuthMessage<C>;

    type Error = Infallible; // @TODO

    fn next_message(
        y: Self::State,
        control_message: &AuthControlMessage<C>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        // @TODO: we aren't focussing on ordering now so no dependencies are required, when we
        // introduce ordering then auth dependencies should be calculated and returned here.
        Ok((
            y,
            AuthMessage::Args(AuthArgs {
                control_message: control_message.clone(),
            }),
        ))
    }

    fn queue(_y: Self::State, _message: &Self::Operation) -> Result<Self::State, Self::Error> {
        // We shift "dependency checked" message ordering to outside of `p2panda-spaces`.
        unreachable!()
    }

    fn next_ready_message(
        _y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error> {
        // We shift "dependency checked" message ordering to outside of `p2panda-spaces`.
        unreachable!()
    }
}

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
    pub(crate) fn from_forged<M: ForgedMessage<C>>(message: &M) -> Self {
        AuthMessage::Forged {
            author: message.author(),
            operation_id: message.id(),
            control_message: AuthControlMessage {
                group_id: message.group_id(),
                action: message.control_message().to_auth_action(),
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
