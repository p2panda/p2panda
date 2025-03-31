// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod acked_dgm;
mod dcgka;
#[cfg(any(test, feature = "test_utils"))]
mod test_utils;
#[cfg(test)]
mod tests;

// TODO: Remove this later.
#[allow(unused)]
pub use dcgka::{
    AckMessage, AddAckMessage, AddMessage, ControlMessage, CreateMessage, Dcgka, DcgkaError,
    DcgkaResult, DcgkaState, DirectMessage, DirectMessageContent, DirectMessageType,
    OperationOutput, ProcessInput, ProcessMessage, ProcessOutput, RemoveMessage, UpdateMessage,
    UpdateSecret,
};
