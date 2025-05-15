// SPDX-License-Identifier: MIT OR Apache-2.0

mod dcgka;
mod group;
mod message;
mod ratchet;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

// TODO: Remove this later.
#[allow(unused)]
pub use dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaResult, DcgkaState, DirectMessage,
    DirectMessageContent, DirectMessageType, OperationOutput, ProcessInput, ProcessOutput,
    UpdateSecret,
};
pub use group::{GroupError, GroupEvent, GroupOutput, GroupState, MessageGroup};
#[allow(unused)]
pub use ratchet::{
    DecryptionRatchet, DecryptionRatchetState, Generation, MESSAGE_KEY_SIZE, RatchetError,
    RatchetSecret, RatchetSecretState,
};
