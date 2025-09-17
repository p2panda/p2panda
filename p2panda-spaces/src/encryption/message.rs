// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::traits::Conditions;
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::{GroupMessage as EncryptionOperation, GroupMessageContent};

use crate::encryption::dgm::EncryptionGroupMembership;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::traits::SpaceId;
use crate::types::{ActorId, EncryptionControlMessage, EncryptionDirectMessage, OperationId};

#[derive(Clone, Debug)]
pub enum EncryptionArgs {
    System {
        dependencies: Vec<OperationId>,
        control_message: EncryptionControlMessage,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Application {
        dependencies: Vec<OperationId>,
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
        ciphertext: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EncryptionMessage {
    Args(EncryptionArgs),
    Forged {
        author: ActorId,
        operation_id: OperationId,
        args: EncryptionArgs,
    },
}

impl EncryptionMessage {
    pub(crate) fn from_membership<ID, M, C>(space_message: &M) -> Vec<Self>
    where
        ID: SpaceId,
        M: AuthoredMessage + SpacesMessage<ID, C>,
        C: Conditions,
    {
        let SpacesArgs::SpaceMembership {
            space_dependencies,
            control_messages,
            ..
        } = space_message.args()
        else {
            panic!("unexpected message type")
        };

        control_messages
            .clone()
            .into_iter()
            .map(|message| {
                let encryption_args = EncryptionArgs::System {
                    dependencies: space_dependencies.clone(),
                    control_message: message.encryption_control_message(),
                    direct_messages: message.direct_messages().to_owned(),
                };
                EncryptionMessage::Forged {
                    author: space_message.author(),
                    operation_id: space_message.id(),
                    args: encryption_args,
                }
            })
            .collect()
    }

    pub(crate) fn from_application<ID, M, C>(space_message: &M) -> Self
    where
        ID: SpaceId,
        M: AuthoredMessage + SpacesMessage<ID, C>,
        C: Conditions,
    {
        let SpacesArgs::Application {
            space_dependencies,
            group_secret_id,
            nonce,
            ciphertext,
            ..
        } = space_message.args()
        else {
            panic!("unexpected message type")
        };

        let encryption_args = EncryptionArgs::Application {
            dependencies: space_dependencies.clone(),
            group_secret_id: *group_secret_id,
            nonce: *nonce,
            ciphertext: ciphertext.to_vec(),
        };

        EncryptionMessage::Forged {
            author: space_message.author(),
            operation_id: space_message.id(),
            args: encryption_args,
        }
    }

    pub(crate) fn dependencies(&self) -> &Vec<OperationId> {
        let args = match self {
            EncryptionMessage::Args(args) => args,
            EncryptionMessage::Forged { args, .. } => args,
        };

        match args {
            EncryptionArgs::System { dependencies, .. } => dependencies,
            EncryptionArgs::Application { dependencies, .. } => dependencies,
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
                ..
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
