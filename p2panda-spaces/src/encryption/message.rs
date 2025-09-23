// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::traits::{Conditions, Operation};
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::{GroupMessage as EncryptionOperation, GroupMessageContent};

use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionGroupMembership;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::space::removed_members;
use crate::traits::SpaceId;
use crate::types::{
    ActorId, AuthGroupAction, EncryptionControlMessage, EncryptionDirectMessage, OperationId,
};

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
    /// Construct an encryption message from a space application message.
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

    /// Construct an encryption message from a corresponding space message and required additional
    /// arguments.
    ///
    /// This method is required when we receive a space message and associated auth message and we
    /// want to adjust our local encryption state accordingly. The main requirement is that we
    /// process our own direct messages (contained in the space message), in many cases the actual
    /// encryption control message type and content is redundant as the DGM state is always
    /// manually replaced with the latest membership state provided by p2panda-auth. The only case
    /// where it does matter is if we ourselves were added or removed from the group, here we
    /// should make sure that the control message contains our own actor id.
    pub(crate) fn from_membership<ID, M, C>(
        space_message: &M,
        my_id: ActorId,
        auth_message: &AuthMessage<C>,
        current_members: Vec<ActorId>,
        next_members: Vec<ActorId>,
    ) -> Self
    where
        ID: SpaceId,
        M: AuthoredMessage + SpacesMessage<ID, C>,
        C: Conditions,
    {
        let SpacesArgs::SpaceMembership {
            space_dependencies,
            auth_message_id,
            direct_messages,
            ..
        } = space_message.args()
        else {
            panic!("unexpected message type");
        };

        // Sanity check.
        assert_eq!(auth_message.id(), *auth_message_id);

        // Check if there are any direct messages for me.
        let my_direct_message = direct_messages
            .iter()
            .any(|message| message.recipient == my_id);

        let encryption_args = match auth_message.payload().action {
            // The auth message is "create" and so a corresponding "create" encryption control
            // message is constructed containing only the next secret members.
            AuthGroupAction::Create { .. } => {
                let control_message = EncryptionControlMessage::Create {
                    initial_members: next_members,
                };
                EncryptionArgs::System {
                    dependencies: space_dependencies.to_owned(),
                    control_message,
                    direct_messages: direct_messages.clone(),
                }
            }
            // The auth message is "add", if there is a direct message for us then use our ActorId
            // for the added member, otherwise use the added members ActorId. Even if this is a
            // group being added, meaning they won't actual be known to the DCGKA, we can use
            // their id as the only thing we care about is making sure the direct messages are
            // processed.
            AuthGroupAction::Add { member, .. } => {
                let control_message = if my_direct_message {
                    EncryptionControlMessage::Add { added: my_id }
                } else {
                    EncryptionControlMessage::Add { added: member.id() }
                };
                EncryptionArgs::System {
                    dependencies: space_dependencies.to_owned(),
                    control_message,
                    direct_messages: direct_messages.clone(),
                }
            }
            // The auth message is "remove", if we were removed, then use our ActorId for the
            // removed member, otherwise use the ActorId of the actual removed member (which may
            // be an individual or group).
            AuthGroupAction::Remove { member } => {
                let removed = removed_members(current_members, next_members);
                let control_message = if removed.contains(&my_id) {
                    EncryptionControlMessage::Remove { removed: my_id }
                } else {
                    EncryptionControlMessage::Remove {
                        removed: member.id(),
                    }
                };
                EncryptionArgs::System {
                    dependencies: space_dependencies.to_owned(),
                    control_message,
                    direct_messages: direct_messages.clone(),
                }
            }
            _ => unimplemented!(),
        };

        EncryptionMessage::Forged {
            author: space_message.author(),
            operation_id: space_message.id(),
            args: encryption_args,
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
        let args = match self {
            EncryptionMessage::Args(args) => args,
            EncryptionMessage::Forged { args, .. } => args,
        };

        match args {
            EncryptionArgs::System {
                direct_messages, ..
            } => direct_messages.clone(),
            EncryptionArgs::Application { .. } => Vec::new(),
        }
    }
}
