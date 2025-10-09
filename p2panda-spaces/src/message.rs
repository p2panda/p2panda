// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::{crypto::xchacha20::XAeadNonce, key_bundle::LongTermKeyBundle};
use serde::{Deserialize, Serialize};

use crate::types::{ActorId, AuthControlMessage, EncryptionDirectMessage, OperationId};

/// Enum representing all possible message types.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SpacesArgs<ID, C> {
    /// System message, contains key bundle of the given author.
    ///
    /// Note: Applications should check if the key bundle was authored by the sender.
    KeyBundle { key_bundle: LongTermKeyBundle },

    /// System message containing an auth control message.
    Auth {
        /// "Control message" describing group operation ("add member", "remove member", etc.).
        control_message: AuthControlMessage<C>,

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
