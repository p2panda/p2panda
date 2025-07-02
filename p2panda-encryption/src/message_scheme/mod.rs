// SPDX-License-Identifier: MIT OR Apache-2.0

//! Message encryption for groups with post-compromise security and strong forward secrecy using
//! a double ratchet algorithm.
pub mod dcgka;
pub mod group;
mod message;
mod ratchet;
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
