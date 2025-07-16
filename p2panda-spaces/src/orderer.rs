// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_auth::traits::Operation as AuthOperation;
use p2panda_encryption::traits::GroupMessage as EncryptionOperation;

use crate::dgm::EncryptionGroupMembership;
use crate::{
    ActorId, AuthControlMessage, EncryptionControlMessage, EncryptionDirectMessage, OperationId,
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

impl<C> p2panda_auth::traits::Orderer<ActorId, OperationId, AuthControlMessage<C>> for AuthOrderer {
    type State = (); // @TODO

    type Operation = AuthArgs;

    type Error = Infallible; // @TODO

    fn next_message(
        y: Self::State,
        payload: &AuthControlMessage<C>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        todo!()
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

#[derive(Clone)]
pub struct AuthArgs {
    // @TODO: Here we will fill in the "dependencies", control message etc. which will be later
    // used by ForgeArgs.
}

// Nothing of this will ever be called at this stage where we're just preparing the arguments for a
// future message to be forged.
impl<C> AuthOperation<ActorId, OperationId, AuthControlMessage<C>> for AuthArgs {
    fn id(&self) -> OperationId {
        unreachable!()
    }

    fn author(&self) -> ActorId {
        unreachable!()
    }

    fn dependencies(&self) -> Vec<OperationId> {
        unreachable!()
    }

    fn previous(&self) -> Vec<OperationId> {
        unreachable!()
    }

    fn payload(&self) -> AuthControlMessage<C> {
        unreachable!()
    }
}

// ~~~ encryption ~~~

pub struct EncryptionOrderer {}

impl p2panda_encryption::traits::Ordering<ActorId, OperationId, EncryptionGroupMembership>
    for EncryptionOrderer
{
    type State = (); // @TODO

    type Error = Infallible; // @TODO

    type Message = EncryptionArgs;

    fn next_control_message(
        y: Self::State,
        control_message: &EncryptionControlMessage,
        direct_messages: &[EncryptionDirectMessage],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        todo!()
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
        todo!()
    }

    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        todo!()
    }
}

impl EncryptionOperation<ActorId, OperationId, EncryptionGroupMembership> for EncryptionArgs {
    fn id(&self) -> OperationId {
        todo!()
    }

    fn sender(&self) -> ActorId {
        todo!()
    }

    fn content(&self) -> p2panda_encryption::traits::GroupMessageContent<ActorId> {
        todo!()
    }

    fn direct_messages(&self) -> Vec<EncryptionDirectMessage> {
        todo!()
    }
}

#[derive(Clone)]
pub struct EncryptionArgs {
    // @TODO: Here we will fill in the "dependencies", control message etc. which will be later
    // used by ForgeArgs.
}
