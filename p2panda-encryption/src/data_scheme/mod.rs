// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data encryption for groups with post-compromise security and optional forward-secrecy.
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
pub use group::{EncryptionGroup, EncryptionGroupError, GroupOutput, GroupResult, GroupState};
pub use group_secret::{
    GROUP_SECRET_SIZE, GroupSecret, GroupSecretError, GroupSecretId, SecretBundle,
    SecretBundleState,
};
