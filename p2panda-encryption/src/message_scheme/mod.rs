// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message encryption for groups with post-compromise security and strong forward-secrecy using
//! a double ratchet algorithm.
mod dcgka;
mod group;
mod message;
mod ratchet;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

pub use dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaResult, DcgkaState, DirectMessage,
    DirectMessageContent, DirectMessageType, OperationOutput, ProcessInput, ProcessOutput,
    UpdateSecret,
};
pub use group::{GroupError, GroupEvent, GroupOutput, GroupState, MessageGroup};
pub use ratchet::{
    DecryptionRatchet, DecryptionRatchetState, Generation, MESSAGE_KEY_SIZE, RatchetError,
    RatchetSecret, RatchetSecretState,
};
