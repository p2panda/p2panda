// SPDX-License-Identifier: MIT OR Apache-2.0

//! Processor stores used to persist events.
//!
//! An implementation of the [`ProcessorStore`] trait is provided for [`SqliteStore`].
//!
//! [`SqliteStore`]: crate::SqliteStore
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use traits::ProcessorStore;
