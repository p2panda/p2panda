// SPDX-License-Identifier: AGPL-3.0-or-later

//! Materialise data `Instance`s from p2panda `Message`s. Create a causal graph of p2panda `Message`s (aka `Operation`s),
//! reconcile any conflicts, order operations deterministically, and reduce operations into data `Instance`s.

mod dag;
mod error;
mod filter;
mod marshall_entries;

pub use dag::{Edge, Node, DAG};
pub use error::MaterialisationError;
pub use marshall_entries::marshall_entries;
