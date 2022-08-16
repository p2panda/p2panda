// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;

/// Response from calling `publish_entry`.
///
/// Contains the arguments needed for publishing the next entry.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishEntryResponse {
    /// The backlink of the next entry to be published.
    pub(crate) backlink: Option<Hash>,

    /// The skiplink of the next entry to be published.
    pub(crate) skiplink: Option<Hash>,

    /// The sequence number of the next entry to be published.
    pub(crate) seq_num: SeqNum,

    /// The log id of the next entry to be published.
    pub(crate) log_id: LogId,
}

/// The next entry args response values..
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EntryArgsResponse {
    /// The backlink of the next entry to be published.
    pub(crate) backlink: Option<Hash>,

    /// The skiplink of the next entry to be published.
    pub(crate) skiplink: Option<Hash>,

    /// The sequence number of the next entry to be published.
    pub(crate) seq_num: SeqNum,

    /// The log id of the next entry to be published.
    pub(crate) log_id: LogId,
}
