// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use serde::{Deserialize, Serialize};

use crate::auth::message::AuthArgs;
use crate::encryption::message::EncryptionArgs;
use crate::space::secret_members;
use crate::types::{
    ActorId, AuthGroupAction, EncryptionControlMessage, EncryptionDirectMessage, OperationId,
};

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;

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

    /// System message containing a space- or group control message.
    ControlMessage {
        /// Space- or group id.
        id: ActorId,

        /// "Control message" describing group operation ("add member", "remove member", etc.).
        control_message: ControlMessage<C>,

        // @TODO: We eventually want application dependencies here too.
        /// Auth dependencies. These are the latest heads of the global auth control message graph.
        auth_dependencies: Vec<OperationId>,

        /// Encryption dependencies. These are the latest heads of the encryption control
        /// and application message graph.
        encryption_dependencies: Vec<OperationId>,

        /// Encrypted, direct messages to members in the group, used for key agreement.
        direct_messages: Vec<EncryptionDirectMessage>,
    },

    /// Encrypted application message used inside a space.
    Application {
        /// Space this message was encrypted for. Members in that space should be able to decrypt
        /// it.
        space_id: ActorId,

        /// Used key id for AEAD.
        group_secret_id: GroupSecretId,

        // @TODO: We probably also want auth dependencies here too.
        // auth_dependencies: Vec<OperationId>,
        /// Encryption dependencies. These are the latest heads of the encryption control
        /// and application message graph.
        encryption_dependencies: Vec<OperationId>,

        /// Used nonce for AEAD.
        nonce: XAeadNonce,

        /// Encrypted application data.
        ciphertext: Vec<u8>,
    },
}

impl<C> SpacesArgs<C>
where
    C: Conditions,
{
    pub(crate) fn from_args(
        group_id: ActorId,
        auth_args: Option<AuthArgs<C>>,
        encryption_args: Option<EncryptionArgs>,
    ) -> Self {
        let (encryption_action, encryption_dependencies, direct_messages) = match encryption_args {
            Some(EncryptionArgs::System {
                control_message,
                dependencies,
                direct_messages,
            }) => (Some(control_message), dependencies, direct_messages),
            None => (None, Vec::new(), Vec::new()),
            Some(EncryptionArgs::Application {
                dependencies,
                group_secret_id,
                nonce,
                ciphertext,
            }) => {
                return Self::from_application_args(
                    group_id,
                    group_secret_id,
                    nonce,
                    ciphertext,
                    dependencies,
                );
            }
        };

        let (auth_action, auth_dependencies) = match auth_args {
            Some(args) => (Some(args.control_message.action), args.dependencies),
            None => (None, vec![]),
        };

        match (auth_action, encryption_action) {
            (None, Some(encryption_control_message)) => {
                Self::from_encryption_args(group_id, encryption_control_message, direct_messages)
            }
            (Some(auth_action), None) => {
                Self::from_auth_args(group_id, auth_action, auth_dependencies)
            }
            (Some(auth_action), Some(encryption_control_message)) => Self::from_both_args(
                group_id,
                auth_action,
                auth_dependencies,
                encryption_control_message,
                encryption_dependencies,
                direct_messages,
            ),
            _ => panic!("invalid arguments"),
        }
    }

    fn from_application_args(
        space_id: ActorId,
        group_secret_id: GroupSecretId,
        nonce: XAeadNonce,
        ciphertext: Vec<u8>,
        encryption_dependencies: Vec<OperationId>,
    ) -> Self {
        Self::Application {
            space_id,
            group_secret_id,
            nonce,
            ciphertext,
            encryption_dependencies,
        }
    }

    fn from_auth_args(
        group_id: ActorId,
        auth_action: AuthGroupAction<C>,
        auth_dependencies: Vec<OperationId>,
    ) -> Self {
        Self::ControlMessage {
            id: group_id,
            control_message: ControlMessage::from_auth_action(&auth_action),
            auth_dependencies,
            encryption_dependencies: vec![],
            direct_messages: vec![],
        }
    }

    // @TODO: Handle encryption-only cases ("update")
    fn from_encryption_args(
        _group_id: ActorId,
        _control_message: EncryptionControlMessage,
        _direct_messages: Vec<EncryptionDirectMessage>,
    ) -> Self {
        todo!();
    }

    fn from_both_args(
        group_id: ActorId,
        auth_action: AuthGroupAction<C>,
        auth_dependencies: Vec<OperationId>,
        encryption_control_message: EncryptionControlMessage,
        encryption_dependencies: Vec<OperationId>,
        direct_messages: Vec<EncryptionDirectMessage>,
    ) -> Self {
        let control_message = match (&auth_action, &encryption_control_message) {
            (AuthGroupAction::Create { .. }, EncryptionControlMessage::Create { .. }) => {
                ControlMessage::from_auth_action(&auth_action)
            }
            (AuthGroupAction::Add { .. }, EncryptionControlMessage::Add { .. }) => {
                ControlMessage::from_auth_action(&auth_action)
            }
            _ => unimplemented!(), // @TODO: More cases will go here. Panic on invalid ones.
        };

        Self::ControlMessage {
            id: group_id,
            control_message,
            auth_dependencies,
            encryption_dependencies,
            direct_messages,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
// @TODO: Clone is a requirement in reflection where SpacesArgs are implemented
// as p2panda Extensions where Clone is a trait bound. Because of this Clone
// needed to be moved out from behind the test_utils feature flag.
//
// #[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub enum ControlMessage<C> {
    Create {
        // GroupMember is required for understanding if a public key / actor id is an individual or
        // a group in case we're adding something with only pull-access. In that case that actor
        // doesn't need to publish a key bundle and every receiver will not strictly be able to
        // verify if it's _really_ a group or individual.
        //
        // In any other case we always want to verify if the group member type is correct.
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    },
    Add {
        member: GroupMember<ActorId>,

        access: Access<C>,
    },
    // @TODO: introduce all other variants.
}

impl<C> ControlMessage<C>
where
    C: Conditions,
{
    pub fn is_create(&self) -> bool {
        matches!(self, ControlMessage::Create { .. })
    }

    pub(crate) fn to_auth_action(&self) -> AuthGroupAction<C> {
        match self {
            ControlMessage::Create { initial_members } => AuthGroupAction::Create {
                initial_members: initial_members.to_owned(),
            },
            ControlMessage::Add { member, access } => AuthGroupAction::Add {
                member: member.to_owned(),
                access: access.to_owned(),
            },
        }
    }

    pub(crate) fn to_encryption_control_message(&self) -> Option<EncryptionControlMessage> {
        match self {
            ControlMessage::Create { initial_members } => Some(EncryptionControlMessage::Create {
                // @TODO: Compute set of members looking at auth state to take transitive
                // membership into account.
                initial_members: secret_members(
                    initial_members
                        .iter()
                        .map(|(member, access)| (member.id(), access.clone()))
                        .collect(),
                ),
            }),
            ControlMessage::Add { member, access } if !access.is_pull() => {
                // @TODO: Need to look into auth state to compute set of added
                // secret member when a group was added and we need to compute
                // transitive members.

                // @TODO: We want to ignore any members which only have "pull" access.
                Some(EncryptionControlMessage::Add { added: member.id() })
            }
            ControlMessage::Add { access, .. } if access.is_pull() => None,
            _ => unimplemented!(),
        }
    }

    pub(crate) fn from_auth_action(action: &AuthGroupAction<C>) -> Self {
        match action.clone() {
            AuthGroupAction::Create { initial_members } => {
                ControlMessage::Create { initial_members }
            }
            AuthGroupAction::Add { member, access } => ControlMessage::Add { member, access },
            _ => unimplemented!(), // @TODO: More cases will go here. Panic on invalid ones.
        }
    }
}
