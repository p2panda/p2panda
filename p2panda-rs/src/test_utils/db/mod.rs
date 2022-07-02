// SPDX-License-Identifier: AGPL-3.0-or-later

mod provider;
mod stores;
mod types;

pub use provider::SimplestStorageProvider;
pub use types::{
    EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse, StorageEntry,
    StorageLog,
};
