// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;
use crate::storage_provider::traits::{AsEntryArgsResponse, AsPublishEntryResponse};

#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryResponse {
    pub backlink: Option<Hash>,
    pub skiplink: Option<Hash>,
    pub seq_num: SeqNum,
    pub log_id: LogId,
}

impl AsPublishEntryResponse for PublishEntryResponse {
    /// Just the constructor method is defined here as all we need this trait for
    /// is constructing entry args to be returned from the default trait methods.
    fn new(backlink: Option<Hash>, skiplink: Option<Hash>, seq_num: SeqNum, log_id: LogId) -> Self {
        Self {
            backlink,
            skiplink,
            seq_num,
            log_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryArgsResponse {
    pub backlink: Option<Hash>,
    pub skiplink: Option<Hash>,
    pub seq_num: SeqNum,
    pub log_id: LogId,
}

impl AsEntryArgsResponse for EntryArgsResponse {
    /// Just the constructor method is defined here as all we need this trait for
    /// is constructing entry args to be returned from the default trait methods.
    fn new(backlink: Option<Hash>, skiplink: Option<Hash>, seq_num: SeqNum, log_id: LogId) -> Self {
        Self {
            backlink,
            skiplink,
            seq_num,
            log_id,
        }
    }
}
