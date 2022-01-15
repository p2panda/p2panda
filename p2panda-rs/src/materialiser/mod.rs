// SPDX-License-Identifier: AGPL-3.0-or-later

//! Materialise data instances from p2panda operations.
//!
//! Create a causal graph of p2panda operations reconcile branches, solve version conflicts
//! automatically, order operations deterministically and reduce them into data instances.
mod dag;
mod error;
mod filter;
mod graph;
mod marshall_entries;

pub use dag::{Edge, Node, DAG};
pub use error::MaterialisationError;
pub use marshall_entries::marshall_entries;
