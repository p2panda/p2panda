// SPDX-License-Identifier: MIT OR Apache-2.0

//! Topic to application data mapping stores.
//!
//! An implementation of the [`TopicStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(test)]
mod tests;
mod traits;

pub use traits::TopicStore;
