// SPDX-License-Identifier: MIT OR Apache-2.0

//! Causal / partial order over a set of items which form a dependency graph.
#[allow(clippy::module_inception)]
mod orderer;
mod processor;
#[cfg(test)]
mod tests;
mod traits;

pub use orderer::CausalOrderer;
pub use processor::{Orderer, OrdererArgs, OrdererError, OrdererMetadata, OrdererResult};
pub use traits::Ordering;
