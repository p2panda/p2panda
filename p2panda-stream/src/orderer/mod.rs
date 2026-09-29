// SPDX-License-Identifier: MIT OR Apache-2.0

/// Causally order items which form a dependency graph.
#[allow(clippy::module_inception)]
mod orderer;
mod processor;
#[cfg(test)]
mod tests;

pub(crate) use orderer::CausalOrderer;
pub use processor::{Orderer, OrdererArgs, OrdererError, OrdererMetadata, OrdererResult};
