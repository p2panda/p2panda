// SPDX-License-Identifier: MIT OR Apache-2.0

//! Establish encrypted, direct connections over Internet Protocol with QUIC.
mod actors;
mod api;
mod builder;
mod config;
mod discovery;
#[cfg(feature = "supervisor")]
mod supervisor;
#[cfg(test)]
mod tests;
pub(crate) mod user_data;

// Re-export useful iroh types.
pub use iroh;
pub use iroh::{EndpointAddr, RelayUrl};

pub use api::{Endpoint, EndpointError};
pub use builder::Builder;
pub use config::IrohConfig;
