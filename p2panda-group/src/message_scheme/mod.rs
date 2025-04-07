// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod acked_dgm;
mod dcgka;
mod ratchet;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

// TODO: Remove this later.
#[allow(unused)]
pub use dcgka::{
    AckMessage, AddAckMessage, AddMessage, ControlMessage, CreateMessage, Dcgka, DcgkaError,
    DcgkaResult, DcgkaState, DirectMessage, DirectMessageContent, DirectMessageType,
    OperationOutput, ProcessInput, ProcessOutput, RemoveMessage, UpdateMessage, UpdateSecret,
};
#[allow(unused)]
pub use ratchet::{MESSAGE_KEY_SIZE, Ratchet, RatchetCiphertext, RatchetError, RatchetState};
