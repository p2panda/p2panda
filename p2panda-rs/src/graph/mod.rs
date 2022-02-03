// SPDX-License-Identifier: AGPL-3.0-or-later

//! Materialise data instances from p2panda operations.
//!
//! Create a causal graph of p2panda operations reconcile branches, solve version conflicts
//! automatically, order operations deterministically and reduce them into data instances.
mod error;
#[allow(clippy::module_inception)]
mod graph;

pub use error::GraphError;
pub use graph::Graph;
