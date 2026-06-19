// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::LazyLock;

use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::hash::HASH_LEN;
use p2panda_core::identity::VERIFYING_KEY_LEN;
use p2panda_core::{Hash, VerifyingKey};
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::traits::{GroupMessage as EncryptionOperation, GroupMessageContent};
use serde::{Deserialize, Serialize};

use crate::auth::message::AuthMessage;
use crate::encryption::dgm::EncryptionGroupMembership;
use crate::message::{ApplicationMessage, SpaceMembershipMessage};
use crate::types::{
    ActorId, AuthGroupAction, EncryptionControlMessage, EncryptionDirectMessage, OperationId,
};
use crate::utils::removed_members;

/// Arguments which are returned from p2panda-encryption APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Message which can be processed by p2panda-encryption APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub(crate) fn from_application(space_message: &ApplicationMessage) -> Self {
        let ApplicationMessage {
            id,
            author,
            space_dependencies,
            group_secret_id,
            nonce,
            ciphertext,
            ..
        } = space_message;

        let encryption_args = EncryptionArgs::Application {
            dependencies: space_dependencies.clone(),
            group_secret_id: *group_secret_id,
            nonce: *nonce,
            ciphertext: ciphertext.to_vec(),
        };

        EncryptionMessage::Forged {
            author: *author,
            operation_id: *id,
            args: encryption_args,
        }
    }

    /// Construct an encryption message from a corresponding space message and required additional
    /// arguments.
    ///
    /// This method is required when we receive a space message and associated auth message and we
    /// want to adjust our local encryption state accordingly. The main requirement is that we
    /// process our own direct messages (contained in the space message); in many cases the actual
    /// encryption control message type and content is redundant as the DGM state is always
    /// manually replaced with the latest membership state provided by p2panda-auth. The only case
    /// where it does matter is if we ourselves were added or removed from the group; here we
    /// should make sure that the control message contains our own actor id.
    pub(crate) fn from_membership<C>(
        space_message: &SpaceMembershipMessage,
        my_id: ActorId,
        auth_message: &AuthMessage<C>,
        current_members: &Vec<ActorId>,
        next_members: &Vec<ActorId>,
    ) -> Self
    where
        C: Conditions,
    {
        let SpaceMembershipMessage {
            id,
            author,
            space_dependencies,
            auth_message_id,
            direct_messages,
            ..
        } = space_message;

        // Sanity check.
        assert_eq!(auth_message.id(), *auth_message_id);

        // Check if there are any direct messages for me.
        let hash_my_direct_messages = direct_messages
            .iter()
            .any(|message| message.recipient == my_id);

        let encryption_args = match auth_message.action() {
            // The auth message is "create" and so a corresponding "create" encryption control
            // message is constructed containing only the next secret members.
            AuthGroupAction::Create { .. } => {
                let control_message = EncryptionControlMessage::Create {
                    initial_members: next_members.to_owned(),
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
                let control_message = if hash_my_direct_messages {
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
                let removed = removed_members(current_members.to_owned(), next_members.to_owned());
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
            author: *author,
            operation_id: *id,
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
                hash_placeholder()
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
                verifying_key_placeholder()
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

// When processing locally created operations we handle unsigned messages where the actor id is not
// known and not required. In these cases we need to satisfy the trait interfaces using a
// placeholder value.
fn verifying_key_placeholder() -> VerifyingKey {
    static PLACEHOLDER_PUBLIC_KEY: LazyLock<VerifyingKey> = LazyLock::new(|| {
        VerifyingKey::from_bytes(&[0; VERIFYING_KEY_LEN])
            .expect("can create public key from constant bytes")
    });
    *PLACEHOLDER_PUBLIC_KEY
}

// When processing locally created operations we handle unsigned messages where the operation id is
// not known and not required. In these cases we need to satisfy the trait interfaces using a
// placeholder value.
fn hash_placeholder() -> Hash {
    static PLACEHOLDER_ID: Hash = Hash::from_bytes([0; HASH_LEN]);
    PLACEHOLDER_ID
}
