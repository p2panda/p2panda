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
mod test_utils;

pub use errors::ValidationError;
pub use requests::{AsEntryArgsRequest, AsPublishEntryRequest};
pub use responses::{AsEntryArgsResponse, AsPublishEntryResponse};
pub use storage_provider::StorageProvider;
