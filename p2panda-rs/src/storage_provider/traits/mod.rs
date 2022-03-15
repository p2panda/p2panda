// SPDX-License-Identifier: AGPL-3.0-or-later

//! Traits used when implementing a custom storage provider for a p2panda client or node.

mod models;
mod requests;
mod responses;
mod storage;

pub use models::{AsStorageEntry, AsStorageLog};
pub use requests::{AsEntryArgsRequest, AsPublishEntryRequest};
pub use responses::{AsEntryArgsResponse, AsPublishEntryResponse};
pub use storage::{EntryStore, LogStore, StorageProvider};
