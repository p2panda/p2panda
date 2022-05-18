// SPDX-License-Identifier: AGPL-3.0-or-later

//! Storage provider traits needed for implementing custom p2panda storage solutions.
pub mod entry;
pub mod errors;
pub mod log;
pub mod operation;
mod requests;
mod responses;
mod storage_provider;
#[cfg(test)]
mod test_provider;

pub use errors::ValidationError;
pub use requests::{AsEntryArgsRequest, AsPublishEntryRequest};
pub use responses::{AsEntryArgsResponse, AsPublishEntryResponse};
#[cfg(test)]
pub use test_provider::{SimplestStorageProvider, StorageEntry, StorageLog};
