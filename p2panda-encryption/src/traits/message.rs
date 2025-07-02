// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

use crate::crypto::xchacha20::XAeadNonce;
#[cfg(any(test, feature = "data_scheme"))]
use crate::data_scheme::{self, GroupSecretId};
#[cfg(any(test, feature = "message_scheme"))]
use crate::message_scheme::{self, Generation};
#[cfg(any(test, feature = "message_scheme"))]
use crate::traits::AckedGroupMembership;
#[cfg(any(test, feature = "data_scheme"))]
use crate::traits::GroupMembership;

/// Interface to express required information from messages following the "data encryption"
/// protocol for groups.
///
/// Applications implementing these traits should authenticate the original sender of each message.
///
/// Messages, except of the direct ones, need to be broadcast to the whole group.
#[cfg(any(test, feature = "data_scheme"))]
pub trait GroupMessage<ID, OP, DGM>
where
    DGM: GroupMembership<ID, OP>,
{
    /// Unique identifier of this message.
    fn id(&self) -> OP;

    /// Unique identifier of the sender of this message.
    fn sender(&self) -> ID;

    /// Returns content of either a control- or application message.
    fn content(&self) -> GroupMessageContent<ID>;

    /// Returns optional list of direct messages.
    fn direct_messages(&self) -> Vec<data_scheme::DirectMessage<ID, OP, DGM>>;
}

#[cfg(any(test, feature = "data_scheme"))]
#[derive(Debug)]
pub enum GroupMessageContent<ID> {
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

#[cfg(any(test, feature = "data_scheme"))]
impl<ID> Display for GroupMessageContent<ID> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Control(control_message) => control_message.to_string(),
                Self::Application {
                    group_secret_id, ..
                } => format!("application @{}", hex::encode(group_secret_id)),
            }
        )
    }
}

/// Interface to express required information from messages following the "message encryption"
/// protocol for groups.
///
/// Applications implementing these traits should authenticate the original sender of each message.
///
/// Messages, except for the direct ones, need to be broadcast to the whole group.
#[cfg(any(test, feature = "message_scheme"))]
pub trait ForwardSecureGroupMessage<ID, OP, DGM>
where
    DGM: AckedGroupMembership<ID, OP>,
{
    /// Unique identifier of this message.
    fn id(&self) -> OP;

    /// Unique identifier of the sender of this message.
    fn sender(&self) -> ID;

    /// Returns data required to manage group encryption and receive decrypted application messages.
    fn content(&self) -> ForwardSecureMessageContent<ID, OP>;

    /// Returns optional list of direct messages.
    ///
    /// Direct messages do not need to be encoded as part of one broadcast message. Applications
    /// can also decide to keep control messages and direct messages detached and use
    /// `ForwardSecureMessage` as a way to express which control message belonged to this set of
    /// direct messages.
    fn direct_messages(&self) -> Vec<message_scheme::DirectMessage<ID, OP, DGM>>;
}

#[cfg(any(test, feature = "message_scheme"))]
#[derive(Debug)]
pub enum ForwardSecureMessageContent<ID, OP> {
    /// Control message managing messaging encryption group.
    Control(message_scheme::ControlMessage<ID, OP>),

    /// Encrypted application message payload indicating which ratchet generation was used.
    Application {
        ciphertext: Vec<u8>,
        generation: Generation,
    },
}

#[cfg(any(test, feature = "message_scheme"))]
impl<ID, OP> Display for ForwardSecureMessageContent<ID, OP> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Control(control_message) => control_message.to_string(),
                Self::Application { generation, .. } => format!("application @{}", generation),
            }
        )
    }
}
