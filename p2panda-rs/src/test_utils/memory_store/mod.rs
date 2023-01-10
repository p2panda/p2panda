// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs and methods for validating, publishing, storing and retrieving p2panda data.
//!
//! - `MemoryStore` implementation of all storage provider traits
//! - `domain` and `validation` modules for publishing and validating entries and operations
//! - helpers for populating a store with test data
pub mod domain;
pub mod helpers;
mod provider;
mod stores;
mod types;
pub mod validation;

pub use provider::MemoryStore;
pub use types::{EntryArgsResponse, PublishEntryResponse, PublishedOperation, StorageEntry};
