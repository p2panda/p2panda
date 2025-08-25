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

impl<M> Default for EncryptionOrderer<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> EncryptionOrderer<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EncryptionOrdererState {
    next_message: Option<EncryptionMessage>,

    // @TODO: logic for keeping these dependencies up-to-date with processed and published
    // operations is not implemented.
    pub dependencies: Vec<OperationId>,
}

impl Default for EncryptionOrdererState {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptionOrdererState {
    pub fn new() -> Self {
        Self {
            next_message: None,

            // @TODO: We should maintain (update) this state for the encryption control message
            // DAG inside of p2panda-spaces. For reference see
            // https://github.com/p2panda/p2panda/blob/main/p2panda-encryption/src/data_scheme/test_utils/ordering.rs#L54-L55
            //
            // @TODO: We don't look at application message dependencies quite yet. More research
            // needed into requirements around bi-directional dependencies between dags.
            dependencies: Default::default(),
        }
    }
}

impl<M> p2panda_encryption::traits::Ordering<ActorId, OperationId, EncryptionGroupMembership>
    for EncryptionOrderer<M>
{
    type State = EncryptionOrdererState;

    type Error = Infallible; // @TODO

    type Message = EncryptionMessage;

    fn next_control_message(
        y: Self::State,
        control_message: &EncryptionControlMessage,
        direct_messages: &[EncryptionDirectMessage],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let dependencies = y.dependencies.clone();
        Ok((
            y,
            EncryptionMessage::Args(EncryptionArgs::System {
                dependencies,
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
        let dependencies = y.dependencies.clone();
        Ok((
            y,
            EncryptionMessage::Args(EncryptionArgs::Application {
                dependencies,
                group_secret_id,
                nonce,
                ciphertext,
            }),
        ))
    }

    fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        y.next_message = Some(message.clone());
        Ok(y)
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
        mut y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        let message = y.next_message.take();
        Ok((y, message))
    }
}
