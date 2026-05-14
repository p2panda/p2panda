// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dependency orderer stores.
//!
//! An implementation of the [`OrdererStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::OrdererStore;
#[cfg(any(test, feature = "test_utils"))]
pub use traits::OrdererTestExt;
