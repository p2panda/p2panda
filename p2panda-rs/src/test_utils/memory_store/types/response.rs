// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;

/// Response from calling `publish_entry`.
///
/// Contains the arguments needed for publishing the next entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryResponse {
    /// The backlink of the next entry to be published.
    pub backlink: Option<Hash>,

    /// The skiplink of the next entry to be published.
    pub skiplink: Option<Hash>,

    /// The sequence number of the next entry to be published.
    pub seq_num: SeqNum,

    /// The log id of the next entry to be published.
    pub log_id: LogId,
}

impl PublishEntryResponse {
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

/// The next entry args response values..
#[derive(Debug, Clone, PartialEq)]
pub struct EntryArgsResponse {
    /// The backlink of the next entry to be published.
    pub backlink: Option<Hash>,

    /// The skiplink of the next entry to be published.
    pub skiplink: Option<Hash>,

    /// The sequence number of the next entry to be published.
    pub seq_num: SeqNum,

    /// The log id of the next entry to be published.
    pub log_id: LogId,
}

impl EntryArgsResponse {
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
