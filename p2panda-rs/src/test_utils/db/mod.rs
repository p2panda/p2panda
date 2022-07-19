// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implementations of all `StorageProvider` traits.
//!
//! Used in the mock node and for testing.
mod provider;
mod stores;
mod types;

pub use provider::MemoryStore;
pub use types::{
    EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse, StorageEntry,
    StorageLog,
};
