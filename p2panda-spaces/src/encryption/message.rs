// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::{GroupMessage as EncryptionOperation, GroupMessageContent};

use crate::encryption::dgm::EncryptionGroupMembership;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::types::{
    ActorId, Conditions, EncryptionControlMessage, EncryptionDirectMessage, OperationId,
};

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
pub enum EncryptionMessage {
    Args(EncryptionArgs),
    Forged {
        author: ActorId,
        operation_id: OperationId,
        args: EncryptionArgs,
    },
}

impl EncryptionMessage {
    pub(crate) fn from_forged<M, C>(message: &M) -> Self
    where
        M: AuthoredMessage + SpacesMessage<C>,
        C: Conditions,
    {
        let args = match message.args() {
            SpacesArgs::ControlMessage {
                control_message,
                direct_messages,
                ..
            } => EncryptionArgs::System {
                control_message: control_message.to_encryption_control_message(),
                direct_messages: direct_messages.to_vec(),
            },
            SpacesArgs::Application {
                group_secret_id,
                nonce,
                ciphertext,
                ..
            } => EncryptionArgs::Application {
                group_secret_id: *group_secret_id,
                nonce: *nonce,
                ciphertext: ciphertext.to_vec(),
            },
            _ => unreachable!("unexpected message type"),
        };

        EncryptionMessage::Forged {
            author: message.author(),
            operation_id: message.id(),
            args,
        }
    }
}

impl EncryptionOperation<ActorId, OperationId, EncryptionGroupMembership> for EncryptionMessage {
    fn id(&self) -> OperationId {
        match self {
            EncryptionMessage::Args(_) => {
                // Our design uses `p2panda_auth` instead of the DGM inside the encryption group
                // API. The DGM is the only part in need of an operation id, so we can give it a
                // placeholder instead.
                OperationId::placeholder()
            }
            EncryptionMessage::Forged { operation_id, .. } => *operation_id,
        }
    }

    fn sender(&self) -> ActorId {
        match self {
            EncryptionMessage::Args(_) => {
                // Our design uses `p2panda_auth` instead of the DGM inside the encryption group
                // API. The DGM is the only part in need of a sender, so we can give it a
                // placeholder instead.
                ActorId::placeholder()
            }
            EncryptionMessage::Forged { author, .. } => *author,
        }
    }

    fn content(&self) -> GroupMessageContent<ActorId> {
        let EncryptionMessage::Forged { args, .. } = self else {
            // Nothing of this will ever be called at this stage where we're just preparing the
            // arguments for a future message to be forged.
            unreachable!();
        };

        match args {
            EncryptionArgs::System {
                control_message, ..
            } => GroupMessageContent::Control(control_message.clone()),
            EncryptionArgs::Application {
                group_secret_id,
                nonce,
                ciphertext,
            } => GroupMessageContent::Application {
                group_secret_id: *group_secret_id,
                nonce: *nonce,
                ciphertext: ciphertext.to_vec(),
            },
        }
    }

    fn direct_messages(&self) -> Vec<EncryptionDirectMessage> {
        let EncryptionMessage::Forged { args, .. } = self else {
            // Nothing of this will ever be called at this stage where we're just preparing the
            // arguments for a future message to be forged.
            unreachable!();
        };

        match args {
            EncryptionArgs::System {
                direct_messages, ..
            } => direct_messages.clone(),
            EncryptionArgs::Application { .. } => Vec::new(),
        }
    }
}
