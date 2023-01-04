// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs and methods for interacting with the `StorageProvider` traits.
//! 
//! - `MemoryStore` implementation of all storage provider traits
//! - `domain` and `validation` methods for publishing and validating entries and operations
//! - helpers for populating a store with test data
pub mod domain;
mod provider;
mod stores;
pub mod helpers;
mod types;
pub mod validation;

pub use provider::MemoryStore;
pub use types::{EntryArgsResponse, PublishEntryResponse, PublishedOperation, StorageEntry};
