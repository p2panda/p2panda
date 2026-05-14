// SPDX-License-Identifier: MIT OR Apache-2.0

//! Operation stores.
//!
//! An implementation of the [`OperationStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::OperationStore;

#[cfg(feature = "sqlite")]
pub(crate) use sqlite::OperationRow;
