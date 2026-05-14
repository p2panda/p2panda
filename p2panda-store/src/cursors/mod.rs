// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cursor position stores.
//!
//! An implementation of the [`CursorStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::CursorStore;
