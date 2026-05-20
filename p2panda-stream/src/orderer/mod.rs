// SPDX-License-Identifier: MIT OR Apache-2.0

//! Causal / partial order over a set of items which form a dependency graph.
#[allow(clippy::module_inception)]
mod orderer;
mod processor;
#[cfg(test)]
mod tests;
mod traits;

use orderer::CausalOrderer;
pub use processor::{Orderer, OrdererError};
pub use traits::Ordering;
