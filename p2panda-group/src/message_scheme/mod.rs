// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod acked_dgm;
mod dcgka;
#[cfg(test)]
mod tests;

pub use dcgka::{
    Dcgka, DcgkaError, DcgkaResult, DcgkaState, DirectMessage, DirectMessageContent,
    DirectMessageType, ProcessInput, ProcessMessage, ProcessOutput,
};
