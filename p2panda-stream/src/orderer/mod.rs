// SPDX-License-Identifier: MIT OR Apache-2.0

//! Causal / partial order over a set of items which form a dependency graph.
#[allow(clippy::module_inception)]
mod orderer;
mod processor;
#[cfg(test)]
mod tests;
mod traits;

// TODO: This will be made private as soon as we've integrated the auth / groups processor into
// p2panda-stream and doesn't need to appear in the docs.
#[doc(hidden)]
pub use orderer::CausalOrderer;
pub use processor::{Orderer, OrdererError};
pub use traits::Ordering;
