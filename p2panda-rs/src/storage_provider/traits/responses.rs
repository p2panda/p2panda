// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::next::entry::{LogId, SeqNum};
use crate::next::hash::Hash;

/// Response when requesting arguments to create a new entry.
#[async_trait]
pub trait AsEntryArgsResponse {
    /// Just the constructor method is defined here as all we need this trait for is constructing
    /// entry arguments to be returned from the default trait methods.
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self;
}

/// Response when publishing a new entry.
#[async_trait]
pub trait AsPublishEntryResponse {
    /// Just the constructor method is defined here as all we need this trait for is constructing
    /// entry args to be returned from the default trait methods.
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self;
}
