// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implementation of `storage_provider` traits for an in memory store.
//!
//! - `MemoryStore` implementation of all storage provider traits
//! - helpers for populating a store with test data
pub mod helpers;
mod provider;
mod stores;
mod types;

pub use provider::MemoryStore;
pub use types::{PublishedOperation, StorageEntry};
