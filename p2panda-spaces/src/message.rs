// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::GroupSecretId;
use serde::{Deserialize, Serialize};

use crate::types::{ActorId, AuthControlMessage, EncryptionDirectMessage, OperationId};

// @TODO: This could be an interesting trait for `p2panda-core`, next to another one where we
// declare dependencies.
pub trait AuthoredMessage: Debug {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;

    // @TODO: Do we need a method here to check the signature?
}

pub trait SpacesMessage<ID, C> {
    fn args(&self) -> &SpacesArgs<ID, C>;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SpacesArgs<ID, C> {
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
