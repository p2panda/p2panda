// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::marker::PhantomData;

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;

use crate::encryption::dgm::EncryptionGroupMembership;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
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

    type Message = EncryptionMessage;

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
        // @TODO: We need to make the orderer aware of the welcome state and only "ready" messages
        // when we are welcomed, otherwise key agreement and decryption might fail.
        //
        // @TODO: We probably also need an error then when someone tries to publish a message in a
        // not-yet-welcomed space.
        Ok(y)
    }

    fn next_ready_message(
        _y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        todo!()
    }
}
