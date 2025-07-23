// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::marker::PhantomData;

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::GroupMessage as EncryptionOperation;

use crate::encryption::dgm::EncryptionGroupMembership;
use crate::types::{ActorId, EncryptionControlMessage, EncryptionDirectMessage, OperationId};

#[derive(Clone, Debug)]
pub struct EncryptionOrderer<M> {
    _marker: PhantomData<M>,
}

impl<M> EncryptionOrderer<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
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
            EncryptionMessage::Args(EncryptionArgs::System {
                control_message: control_message.clone(),
                direct_messages: direct_messages.to_vec(),
            }),
        ))
    }

    fn next_application_message(
        y: Self::State,
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
        ciphertext: Vec<u8>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        // @TODO: we aren't focussing on ordering now so no dependencies are required, when we
        // introduce ordering then encryption dependencies should be calculated and returned here.
        Ok((
            y,
            EncryptionMessage::Args(EncryptionArgs::Application {
                group_secret_id,
                nonce,
                ciphertext,
            }),
        ))
    }

    fn queue(_y: Self::State, _message: &Self::Message) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn set_welcome(y: Self::State, _message: &Self::Message) -> Result<Self::State, Self::Error> {
        // No-op
        Ok(y)
    }

    fn next_ready_message(
        _y: Self::State,
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
pub enum EncryptionArgs {
    // @TODO: Here we will fill in the "dependencies", which will be later used by ForgeArgs.
    System {
        control_message: EncryptionControlMessage,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Application {
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
        ciphertext: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EncryptionMessage<M> {
    Args(EncryptionArgs),
    Forged(M),
}
