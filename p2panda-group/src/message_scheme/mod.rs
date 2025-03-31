// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod acked_dgm;
mod dcgka;
#[cfg(test)]
mod tests;

// TODO: Remove this later.
#[allow(unused)]
pub use dcgka::{
    ControlMessage, Dcgka, DcgkaError, DcgkaResult, DcgkaState, DirectMessage,
    DirectMessageContent, DirectMessageType, OperationOutput, ProcessInput, ProcessMessage,
    ProcessOutput, UpdateSecret,
};
