// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group state stores.
//!
//! An implementation of the [`GroupsStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use traits::GroupsStore;
