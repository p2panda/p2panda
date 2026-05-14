// SPDX-License-Identifier: MIT OR Apache-2.0

//! Append-only log entry stores.
//!
//! An implementation of the [`LogStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use traits::LogStore;
