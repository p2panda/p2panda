// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data encryption for groups with post-compromise security and optional forward secrecy.
//!
//! This "Data Encryption" scheme allows peers to encrypt any data with a secret, symmetric key for
//! a group. This will be useful for building applications where users who enter a group late will
//! still have access to previously-created content, for example private knowledge or wiki
//! applications or a booking tool for rehearsal rooms.
//!
//! A member will not learn about any newly-created data after they are removed from the group,
//! since the key gets rotated on member removal or manual key update. This should accommodate for
//! many use-cases in p2p applications which rely on basic group encryption with post-compromise
//! security (PCS) and forward secrecy (FS) during key agreement.
//!
//! ## Messages
//!
//! Every group operation ([create](EncryptionGroup::create) or [update](EncryptionGroup::update)
//! group, [add](EncryptionGroup::add) or [remove](EncryptionGroup::remove) member) results in a
//! [`ControlMessage`] which is broadcast to the network for each group member, along with a set of direct messages.
//!
//! A [`DirectMessage`] is sent to a specific group member and contains the group secrets encrypted
//! towards them for key agreement.
//!
//! Application messages contain the ciphertexts and parameters required to decrypt it.
//!
//! ## Key agreement and encryption
//!
//! The "Data Encryption" group API is mostly a wrapper around the [2SM (Two-Party Secure
//! Messaging) key agreement protocol](crate::two_party). On creating or updating a group and
//! removing a member, a new, random [`GroupSecret`] is [generated](SecretBundle::generate). Every
//! secret is identified with a unique [`GroupSecretId`] and has a UNIX timestamp indicating when
//! it was created.
//!
//! Peers maintain a [`SecretBundle`] with all group secrets inside. Secrets are added to the
//! bundle when local group operations took place or when learning about a new secret from another
//! member after receiving a control message.
//!
//! When sending new data into the group we [look up the latest secret](SecretBundleState::latest)
//! in the bundle (via comparing timestamps) and use XChaCha20Poly1305 as an AEAD to [encrypt the
//! payload](encrypt_data) with a random nonce. Next to each ciphertext the used nonce and group
//! secret id is mentioned so other members of the group can [decrypt the data](decrypt_data).
//!
//! Members who have been added to the group will learn about the whole secret bundle included in a
//! direct "welcome" message encrypted towards them using the 2SM protocol. Through this the added
//! member will be able to decrypt all previously-created content as they will learn about all
//! used secrets.
//!
//! ## Optional forward secrecy
//!
//! Applications can remove group secrets for forward secrecy based on their own logic. For
//! removing group secrets implementers can use the [`EncryptionGroup::update_secrets`] method.
//!
//! For stronger forward secrecy guarantees have a look at the ["Message
//! Encryption"](crate::message_scheme) scheme.
//!
//! ## Key bundles
//!
//! For initial key agreement (X3DH) peers need to publish key bundles into the network to allow
//! others to invite them into groups. For the "Data Encryption" scheme we're using long-term
//! pre-keys with lifetimes specified by the application.
//!
//! More on key bundles can be read [here](crate::key_bundle).
//!
//! ## Usage
//!
//! Check out the [`EncryptionGroup`] API for establishing and maintaining groups using the "Data
//! Encryption" scheme.
//!
//! [`GroupSecret`] and [`SecretBundle`] are always explicitly passed into every group operation
//! ("add member", "remove member", etc.) to allow full control over the managed keys for
//! applications.
//!
//! Developers need to bring their own data types with [group message
//! interfaces](crate::traits::GroupMessage), [decentralised group
//! membership](crate::traits::GroupMembership) (DGM) and [ordering](crate::traits::Ordering)
//! implementations when using this crate directly. For easier use without this overhead it's
//! recommended to look into higher-level integrations using the p2panda stack.
mod data;
pub mod dcgka;
pub mod group;
pub mod group_secret;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

pub use data::{decrypt_data, encrypt_data};
pub use dcgka::{ControlMessage, DirectMessage, DirectMessageContent, DirectMessageType};
pub use group::{EncryptionGroup, GroupError, GroupOutput, GroupResult, GroupState};
pub use group_secret::{
    GROUP_SECRET_SIZE, GroupSecret, GroupSecretError, GroupSecretId, SecretBundle,
    SecretBundleState,
};
