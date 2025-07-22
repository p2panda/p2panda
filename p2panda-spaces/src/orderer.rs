// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::marker::PhantomData;

use p2panda_auth::group::GroupControlMessage;
use p2panda_auth::traits::Operation as AuthOperation;
use p2panda_encryption::traits::GroupMessage as EncryptionOperation;

use crate::dgm::EncryptionGroupMembership;
use crate::forge::SpacesMessage;
use crate::{
    ActorId, AuthControlMessage, Conditions, EncryptionControlMessage, EncryptionDirectMessage,
    OperationId,
};

// ~~~ auth ~~~

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
    pub(crate) fn from_forged<M: SpacesMessage<C>>(message: M) -> Self {
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
            AuthMessage::Args(auth_args) => {
                // Nothing of this will ever be called at this stage where we're just preparing the
                // arguments for a future message to be forged.
                unimplemented!()
            }
            AuthMessage::Forged { operation_id, .. } => *operation_id,
        }
    }

    fn author(&self) -> ActorId {
        match self {
            AuthMessage::Args(auth_args) => {
                // Nothing of this will ever be called at this stage where we're just preparing the
                // arguments for a future message to be forged.
                unimplemented!()
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
            AuthMessage::Args(auth_args) => {
                // Nothing of this will ever be called at this stage where we're just preparing the
                // arguments for a future message to be forged.
                unimplemented!()
            }
            AuthMessage::Forged {
                control_message, ..
            } => control_message.clone(),
        }
    }
}

// ~~~ encryption ~~~

#[derive(Debug)]
pub struct EncryptionOrderer<M> {
    _phantom: PhantomData<M>,
}

impl<M> EncryptionOrderer<M> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<M> p2panda_encryption::traits::Ordering<ActorId, OperationId, EncryptionGroupMembership>
    for EncryptionOrderer<M>
{
    type State = (); // @TODO

    type Error = Infallible; // @TODO

    type Message = EncryptionMessage<M>;

    fn next_control_message(
        y: Self::State,
        control_message: &EncryptionControlMessage,
        direct_messages: &[EncryptionDirectMessage],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        // @TODO: we aren't focussing on ordering now so no dependencies are required, when we
        // introduce ordering then encryption dependencies should be calculated and returned here.
        Ok((
            y,
            EncryptionMessage::Args(EncryptionArgs {
                control_message: control_message.clone(),
                direct_messages: direct_messages.to_vec(),
            }),
        ))
    }

    fn next_application_message(
        y: Self::State,
        group_secret_id: p2panda_encryption::data_scheme::GroupSecretId,
        nonce: p2panda_encryption::crypto::xchacha20::XAeadNonce,
        ciphertext: Vec<u8>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        todo!()
    }

    fn queue(y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn set_welcome(y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        // No-op
        Ok(y)
    }

    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        todo!()
    }
}

impl<M> EncryptionOperation<ActorId, OperationId, EncryptionGroupMembership>
    for EncryptionMessage<M>
{
    fn id(&self) -> OperationId {
        match self {
            EncryptionMessage::Args(_) => OperationId::placeholder(),
            EncryptionMessage::Forged(_) => todo!(),
        }
    }

    fn sender(&self) -> ActorId {
        match self {
            EncryptionMessage::Args(_) => ActorId::placeholder(),
            EncryptionMessage::Forged(_) => todo!(),
        }
    }

    fn content(&self) -> p2panda_encryption::traits::GroupMessageContent<ActorId> {
        todo!()
    }

    fn direct_messages(&self) -> Vec<EncryptionDirectMessage> {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct EncryptionArgs {
    // @TODO: Here we will fill in the "dependencies", control message etc. which will be later
    // used by ForgeArgs.
    pub(crate) control_message: EncryptionControlMessage,
    pub(crate) direct_messages: Vec<EncryptionDirectMessage>,
}

#[derive(Clone, Debug)]
pub enum EncryptionMessage<M> {
    Args(EncryptionArgs),
    Forged(M),
}
