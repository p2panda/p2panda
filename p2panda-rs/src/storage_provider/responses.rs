// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use serde::Serialize;

use crate::{
    entry::{LogId, SeqNum},
    hash::Hash,
};

/// Response body of `panda_getEntryArguments`.
#[async_trait]
pub trait AsEntryArgsResponse {
    fn new(
        entry_hash_backlink: Option<Hash>,
        entry_hash_skiplink: Option<Hash>,
        seq_num: SeqNum,
        log_id: LogId,
    ) -> Self;
}
