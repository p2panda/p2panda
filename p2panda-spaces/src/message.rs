// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::fmt::Debug;

use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::Conditions;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::{crypto::xchacha20::XAeadNonce, key_bundle::LongTermKeyBundle};
use serde::{Deserialize, Serialize};

use crate::auth::message::AuthMessage;
use crate::traits::{AuthoredMessage, SpaceId};
use crate::types::{ActorId, EncryptionDirectMessage, OperationId};

/// Spaces message type.
///
/// Although the spaces API is generic over concrete data type both when messages are forged
/// (output) and processed (input) this type is used internally where generic types are not
/// required and also exposes an API for converting into specific message variants where these are
/// needed.
#[derive(Clone, Debug)]
pub struct SpacesMessage<SID, C> {
    pub id: OperationId,
    pub author: ActorId,
    pub args: SpacesArgs<SID, C>,
}

impl<SID, C> Borrow<SpacesArgs<SID, C>> for SpacesMessage<SID, C> {
    fn borrow(&self) -> &SpacesArgs<SID, C> {
        &self.args
    }
}

impl<SID, C> AuthoredMessage for SpacesMessage<SID, C> {
    fn id(&self) -> OperationId {
        self.id
    }

    fn author(&self) -> ActorId {
        self.author
    }
}

/// Message type representing a group membership change on a space.
pub(crate) struct SpaceMembershipMessage {
    pub id: OperationId,
    pub author: ActorId,
    pub group_id: ActorId,
    pub space_dependencies: Vec<OperationId>,
    pub auth_message_id: OperationId,
    pub direct_messages: Vec<EncryptionDirectMessage>,
}

/// Message type representing application messages.
pub(crate) struct ApplicationMessage {
    pub id: OperationId,
    pub author: ActorId,
    pub space_dependencies: Vec<OperationId>,
    pub group_secret_id: GroupSecretId,
    pub nonce: XAeadNonce,
    pub ciphertext: Vec<u8>,
}

impl<SID, C> SpacesMessage<SID, C>
where
    SID: SpaceId,
    C: Conditions,
{
    pub(crate) fn space_membership<M>(message: &M) -> SpaceMembershipMessage
    where
        M: AuthoredMessage + Borrow<SpacesArgs<SID, C>>,
    {
        let SpacesArgs::SpaceMembership {
            group_id,
            space_dependencies,
            auth_message_id,
            direct_messages,
            ..
        } = message.borrow().clone()
        else {
            panic!("unexpected message type")
        };
        SpaceMembershipMessage {
            id: message.id(),
            author: message.author(),
            group_id,
            space_dependencies,
            auth_message_id,
            direct_messages,
        }
    }

    pub(crate) fn auth<M>(message: &M) -> AuthMessage<C>
    where
        M: AuthoredMessage + Borrow<SpacesArgs<SID, C>>,
    {
        let SpacesArgs::Auth {
            group_id,
            group_action,
            auth_dependencies,
        } = &message.borrow()
        else {
            panic!("unexpected message type")
        };
        AuthMessage {
            operation_id: message.id(),
            author: message.author(),
            dependencies: auth_dependencies.to_owned(),
            group_id: *group_id,
            action: group_action.to_owned(),
        }
    }

    pub(crate) fn application<M>(message: &M) -> ApplicationMessage
    where
        M: AuthoredMessage + Borrow<SpacesArgs<SID, C>>,
    {
        let SpacesArgs::Application {
            space_dependencies,
            group_secret_id,
            nonce,
            ciphertext,
            ..
        } = message.borrow().to_owned()
        else {
            panic!("unexpected message type")
        };
        ApplicationMessage {
            id: message.id(),
            author: message.author(),
            space_dependencies,
            group_secret_id,
            nonce,
            ciphertext,
        }
    }
}

/// Enum representing all possible message types.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SpacesArgs<ID, C> {
    /// System message, contains key bundle of the given author.
    ///
    /// Note: Applications should check if the key bundle was authored by the sender.
    KeyBundle { key_bundle: LongTermKeyBundle },

    /// System message containing an auth control message.
    Auth {
        /// id of the group this message applies to.
        group_id: ActorId,

        /// Action to be applied to this group.
        group_action: GroupAction<ActorId, C>,

        /// Auth dependencies. These are the latest heads of the global auth control message graph.
        auth_dependencies: Vec<OperationId>,
    },

    /// System message containing a reference to an `SpacesArgs::Auth` message and additional
    /// fields for applying the resulting membership change to a specific space.
    SpaceMembership {
        /// Space this message should be applied to.
        space_id: ID,

        /// Group associated with this space from which group membership is derived.
        group_id: ActorId,

        /// Last known space operation graph tips.
        space_dependencies: Vec<OperationId>,

        /// Reference to (global/shared) auth message which should be applied to the (local) space
        /// state.
        ///
        /// This is a dependency and should be considered when ordering space messages.
        auth_message_id: OperationId,

        /// All direct messages that a local peer generated when processing the referenced auth
        /// message on this space.
        direct_messages: Vec<EncryptionDirectMessage>,
    },

    /// Rotate the entropy for a space's encryption context.
    SpaceUpdate {
        /// Space this message should be applied to.
        space_id: ID,

        /// Group associated with this space from which group membership is derived.
        group_id: ActorId,

        /// Last known space operation graph tips.
        space_dependencies: Vec<OperationId>,
    },

    /// An encrypted application message.
    Application {
        /// Space this message should be applied to.
        space_id: ID,

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

impl<ID, C> SpacesArgs<ID, C> {
    /// Return all dependencies for this spaces message.
    ///
    /// These dependencies can be used to causally order messages before processing them on the
    /// spaces manager. A message should only be processed once all of it' dependencies have
    /// themselves been processed.
    pub fn dependencies(&self) -> Vec<OperationId> {
        match self {
            // @TODO: do key bundles have dependencies?
            SpacesArgs::KeyBundle { .. } => todo!(),
            SpacesArgs::Auth {
                auth_dependencies, ..
            } => auth_dependencies.to_owned(),
            SpacesArgs::SpaceMembership {
                space_dependencies,
                auth_message_id,
                ..
            } => {
                let mut dependencies = vec![*auth_message_id];
                dependencies.extend(space_dependencies.to_owned());
                dependencies
            }
            SpacesArgs::SpaceUpdate {
                space_dependencies, ..
            } => space_dependencies.to_owned(),
            SpacesArgs::Application {
                space_dependencies, ..
            } => space_dependencies.to_owned(),
        }
    }
}
