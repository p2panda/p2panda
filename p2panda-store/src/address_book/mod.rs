// SPDX-License-Identifier: MIT OR Apache-2.0

//! Node information stores.
//!
//! An implementation of the [`AddressBookStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::{AddressBookStore, NodeInfo};
