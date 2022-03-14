// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;

use crate::entry::{EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::OperationEncoded;

/// Request body of `panda_getEntryArguments`.
pub trait AsEntryArgsRequest {
    fn author(&self) -> &Author;
    fn document(&self) -> &Option<Hash>;
}

/// Request body of `panda_publishEntry`.
pub trait AsPublishEntryRequest {
    fn entry_encoded(&self) -> &EntrySigned;
    fn operation_encoded(&self) -> Option<&OperationEncoded>;
}
