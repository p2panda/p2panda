// SPDX-License-Identifier: MIT OR Apache-2.0

//! Key registry stores.
//!
//! An implementation of the [`KeyRegistryStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::KeyRegistryStore;
