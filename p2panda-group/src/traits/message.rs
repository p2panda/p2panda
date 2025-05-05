// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

use crate::crypto::xchacha20::XAeadNonce;
use crate::data_scheme::{self, GroupSecretId};
use crate::message_scheme::{self, Generation};
use crate::traits::{AckedGroupMembership, GroupMembership};

/// Interface to express required information from messages following the "data encryption"
/// protocol for groups.
///
/// Applications implementing these traits should authenticate the original sender of each message.
///
/// Messages, except of the direct ones, need to be broadcast to the whole group.
pub trait GroupMessage<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    /// Unique identifier of this message.
    fn id(&self) -> OP;

    /// Unique identifier of the sender of this message.
    fn sender(&self) -> ID;

    /// Returns if this is a control- or application message.
    fn message_type(&self) -> GroupMessageType<ID>;

    /// Returns optional list of direct messages.
    fn direct_messages(&self) -> Vec<data_scheme::DirectMessage<ID, OP, DGM>>;
}

#[derive(Debug)]
pub enum GroupMessageType<ID> {
    /// Control message managing encryption group.
    Control(data_scheme::ControlMessage<ID>),

    /// Encrypted application payload indicating which AEAD key and nonce was used.
    Application {
        /// Identifier of the used AEAD key (group secret).
        group_secret_id: GroupSecretId,

        /// AEAD nonce.
        nonce: XAeadNonce,

        /// Payload encrypted with AEAD.
        ciphertext: Vec<u8>,
    },
}

impl<ID> Display for GroupMessageType<ID> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Control(control_message) => control_message.to_string(),
            Self::Application {
                group_secret_id, ..
            } => format!("application @{}", hex::encode(group_secret_id)),
        })
    }
}

/// Interface to express required information from messages following the "message encryption"
/// protocol for groups.
///
/// Applications implementing these traits should authenticate the original sender of each message.
///
/// Messages, except for the direct ones, need to be broadcast to the whole group.
pub trait ForwardSecureGroupMessage<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    /// Unique identifier of this message.
    fn id(&self) -> OP;

    /// Unique identifier of the sender of this message.
    fn sender(&self) -> ID;

    /// Returns if this is a control- or application message.
    fn message_type(&self) -> ForwardSecureMessageType<ID, OP>;

    /// Returns optional list of direct messages.
    ///
    /// Direct messages do not need to be encoded as part of one broadcast message. Applications
    /// can also decide to keep control messages and direct messages detached and use
    /// `ForwardSecureMessage` as a way to express which control message belonged to this set of
    /// direct messages.
    fn direct_messages(&self) -> Vec<message_scheme::DirectMessage<ID, OP, DGM>>;
}

#[derive(Debug)]
pub enum ForwardSecureMessageType<ID, OP> {
    /// Control message managing messaging encryption group.
    Control(message_scheme::ControlMessage<ID, OP>),

    /// Encrypted application message payload indicating which ratchet generation was used.
    Application {
        ciphertext: Vec<u8>,
        generation: Generation,
    },
}

impl<ID, OP> Display for ForwardSecureMessageType<ID, OP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Control(control_message) => control_message.to_string(),
            Self::Application { generation, .. } => format!("application @{}", generation),
        })
    }
}
