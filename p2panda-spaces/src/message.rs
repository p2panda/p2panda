// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use serde::{Deserialize, Serialize};

use crate::{
    encryption::message::{EncryptionArgs, EncryptionMessage},
    types::{
        ActorId, AuthControlMessage, EncryptionControlMessage, EncryptionDirectMessage, OperationId,
    },
};

// @TODO: This could be an interesting trait for `p2panda-core`, next to another one where we
// declare dependencies.
pub trait AuthoredMessage: Debug {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;

    // @TODO: Do we need a method here to check the signature?
}

pub trait SpacesMessage<C> {
    fn args(&self) -> &SpacesArgs<C>;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SpacesArgs<C> {
    /// System message, contains key bundle of the given author.
    ///
    /// Note: Applications should check if the key bundle was authored by the sender.
    KeyBundle {
        // @TODO: Key bundle material.
    },
    /// System message containing an auth control message.
    Auth {
        /// "Control message" describing group operation ("add member", "remove member", etc.).
        control_message: AuthControlMessage<C>,

        // @TODO: We eventually want application dependencies here too.
        /// Auth dependencies. These are the latest heads of the global auth control message graph.
        auth_dependencies: Vec<OperationId>,
    },
    SpaceMembership {
        /// Space this message should be applied to.
        space_id: ActorId,

        /// Group associated with this space from which group membership is derived.
        group_id: ActorId,

        /// Last known space operation graph tips.
        space_dependencies: Vec<OperationId>,

        /// Reference to (global/shared) auth message which should be applied to the (local) space
        /// state.
        ///
        /// This is a dependency and should be considered when ordering space messages.
        auth_message_id: OperationId,

        /// The control messages which should be applied to the spaces' group encryption state.
        ///
        /// // @TODO: need to clarify validation requirements when assuring the control messages
        /// // match the related auth message.
        control_messages: Vec<SpaceMembershipControlMessage>,
    },
    SpaceUpdate {
        /// Space this message should be applied to.
        space_id: ActorId,

        /// Group associated with this space from which group membership is derived.
        group_id: ActorId,

        /// Last known space operation graph tips.
        space_dependencies: Vec<OperationId>,
    },
    Application {
        /// Space this message should be applied to.
        space_id: ActorId,

        /// Last known space operation graph tips.
        space_dependencies: Vec<OperationId>,

        /// Used key id for AEAD.
        group_secret_id: GroupSecretId,

        /// Used nonce for AEAD.
        nonce: XAeadNonce,

        /// Encrypted application data.
        ciphertext: Vec<u8>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SpaceMembershipControlMessage {
    Create {
        initial_members: Vec<ActorId>,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Add {
        added: ActorId,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
    Remove {
        removed: ActorId,
        direct_messages: Vec<EncryptionDirectMessage>,
    },
}

impl SpaceMembershipControlMessage {
    pub(crate) fn direct_messages(&self) -> &Vec<EncryptionDirectMessage> {
        match self {
            SpaceMembershipControlMessage::Create {
                direct_messages, ..
            } => direct_messages,
            SpaceMembershipControlMessage::Add {
                direct_messages, ..
            } => direct_messages,
            SpaceMembershipControlMessage::Remove {
                direct_messages, ..
            } => direct_messages,
        }
    }

    pub(crate) fn encryption_control_message(&self) -> EncryptionControlMessage {
        match self.to_owned() {
            SpaceMembershipControlMessage::Create {
                initial_members, ..
            } => EncryptionControlMessage::Create { initial_members },
            SpaceMembershipControlMessage::Add { added, .. } => {
                EncryptionControlMessage::Add { added }
            }
            SpaceMembershipControlMessage::Remove { removed, .. } => {
                EncryptionControlMessage::Remove { removed }
            }
        }
    }

    pub(crate) fn from_encryption_message(encryption_message: &EncryptionMessage) -> Self {
        let EncryptionMessage::Args(args) = encryption_message else {
            panic!("unexpected message type")
        };
        let EncryptionArgs::System {
            control_message,
            direct_messages,
            ..
        } = args.to_owned()
        else {
            panic!("unexpected message type")
        };
        match control_message {
            EncryptionControlMessage::Create { initial_members } => {
                SpaceMembershipControlMessage::Create {
                    initial_members,
                    direct_messages,
                }
            }
            EncryptionControlMessage::Add { added } => SpaceMembershipControlMessage::Add {
                added,
                direct_messages,
            },
            EncryptionControlMessage::Remove { removed } => SpaceMembershipControlMessage::Remove {
                removed,
                direct_messages,
            },
            _ => panic!("unexpected message type"),
        }
    }
}
