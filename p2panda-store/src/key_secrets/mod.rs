// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-key bundle stores.
//!
//! An implementation of the [`KeySecretsStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::KeySecretsStore;
