// SPDX-License-Identifier: MIT OR Apache-2.0

mod data;
mod dcgka;
mod dgm;
mod group;
mod group_secret;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

#[allow(unused)]
pub use dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaResult, DcgkaState, DirectMessage,
    DirectMessageContent, DirectMessageType, OperationOutput, ProcessInput, ProcessOutput,
};
#[allow(unused)]
pub use group_secret::{
    GROUP_SECRET_SIZE, GroupSecret, GroupSecretError, GroupSecretId, SecretBundle,
    SecretBundleState,
};
