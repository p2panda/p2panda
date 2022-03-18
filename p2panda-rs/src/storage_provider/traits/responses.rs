// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;

/// Trait to be implemented on the response body of `panda_getEntryArguments`.
#[async_trait]
pub trait AsEntryArgsResponse {
    /// Just the constructor method is defined here as all we need this trait for
    /// is constructing entry args to be returned from the default trait methods.
    ///
    /// NB: We could validate that entries at seq nums that require skiplinks have them.
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self;
}

/// Trait to be implemented on the response body of `panda_publishEntry`.
#[async_trait]
pub trait AsPublishEntryResponse {
    /// Just the constructor method is defined here as all we need this trait for
    /// is constructing entry args to be returned from the default trait methods.
    ///
    /// NB: We could validate that entries at seq nums that require skiplinks have them.
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self;
}
