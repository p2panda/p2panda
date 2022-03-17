// SPDX-License-Identifier: AGPL-3.0-or-later

//! Traits used when implementing a custom storage provider for a p2panda client or node.

mod entry_store;
mod log_store;
mod models;
mod requests;
mod responses;
mod storage_provider;
#[cfg(test)]
mod test_setup;

pub use entry_store::EntryStore;
pub use log_store::LogStore;
pub use models::{AsStorageEntry, AsStorageLog};
pub use requests::{AsEntryArgsRequest, AsPublishEntryRequest};
pub use responses::{AsEntryArgsResponse, AsPublishEntryResponse};
pub use storage_provider::StorageProvider;
