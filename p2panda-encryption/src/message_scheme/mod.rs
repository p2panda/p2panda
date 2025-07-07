// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message Encryption for groups offering a forward secure (FS) messaging ratchet, similar to
//! Signal's [Double Ratchet algorithm](https://en.wikipedia.org/wiki/Double_Ratchet_Algorithm).
//!
//! Since secret keys are always generated for each message, a user can not easily learn about
//! previously-created messages when getting hold of such a key. We believe that the latter scheme
//! will be used in more specialised applications, for example p2p group chats, as strong forward
//! secrecy comes with it's own UX requirements. We are nonetheless excited to offer a solution for
//! both worlds, depending on the application's needs.
//!
//! ## Messages
//!
//! Every group operation ([create](MessageGroup::create) or [update](MessageGroup::update)
//! group, [add](MessageGroup::add) or [remove](MessageGroup::remove) member) results in a
//! [`ControlMessage`] which is broadcast to the network for each group member and a set of direct
//! messages.
//!
//! A [`DirectMessage`] is sent to a specific group member and contains the group secrets encrypted
//! towards them for key agreement.
//!
//! Application messages contain the ciphertexts and parameters required to decrypt it using a
//! message ratchet.
//!
//! ## Message Ratchets
//!
//! The "inner" message ratchet derives a ChaCha20Poly1305 AEAD secret and nonce for message
//! encryption for each "generation". Each peer maintains their own [`RatchetSecret`] for sending
//! messages and keeps around one [`DecryptionRatchet`] per other member in the group to decrypt
//! received messages from them.
//!
//! Messages arriving out of order or getting lost are tolerated within the [configured
//! limits](group::GroupConfig).
//!
//! ## Key Agreement
//!
//! Extra care is required to keep the chain secrets, updating the "outer" ratchets in a strict, linearized
//! order. Consult the [DCGKA](dcgka) module for more information.
//!
//! ## Key bundles
//!
//! For initial key agreement (X3DH) peers need to publish key bundles into the network to allow
//! others to invite them into groups. For the "Message Encryption" scheme we're using one-time
//! pre-keys which bring additional requirements due to their "only use once" limitation.
//!
//! More on key bundles can be read [here](crate::key_bundle).
//!
//! ## Usage
//!
//! Check out the [`MessageGroup`] API for establishing and maintaining groups using the "Message
//! Encryption" scheme.
//!
//! Developers need to bring their own data types with [group message
//! interfaces](crate::traits::ForwardSecureGroupMessage), [decentralised group
//! membership](crate::traits::AckedGroupMembership) (DGM) and
//! [ordering](crate::traits::ForwardSecureOrdering) implementations when using this crate
//! directly, for easier use without this overhead it's recommended to look into higher-level
//! integrations using the p2panda stack.
pub mod dcgka;
pub mod group;
mod message;
pub mod ratchet;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

pub use dcgka::{ControlMessage, DirectMessage, DirectMessageContent, DirectMessageType};
pub use group::{GroupError, GroupEvent, GroupOutput, GroupState, MessageGroup};
pub use message::{decrypt_message, encrypt_message};
pub use ratchet::{
    DecryptionRatchet, DecryptionRatchetState, Generation, MESSAGE_KEY_SIZE, RatchetError,
    RatchetSecret, RatchetSecretState,
};
