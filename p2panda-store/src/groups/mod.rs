// SPDX-License-Identifier: MIT OR Apache-2.0

//! `GroupsStore` trait for setting and retrieving groups states as well as a concrete
//! `SqliteStore` implementation.
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use traits::GroupsStore;
