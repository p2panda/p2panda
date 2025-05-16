// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data encryption for groups with post-compromise security and optional forward-secrecy.
mod data;
mod dcgka;
mod dgm;
mod group;
mod group_secret;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

pub use dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaResult, DcgkaState, DirectMessage,
    DirectMessageContent, DirectMessageType, OperationOutput, ProcessInput, ProcessOutput,
};
pub use group_secret::{
    GROUP_SECRET_SIZE, GroupSecret, GroupSecretError, GroupSecretId, SecretBundle,
    SecretBundleState,
};
