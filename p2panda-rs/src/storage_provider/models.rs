// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt::Debug;

use super::StorageProviderError;
use crate::entry::EntrySigned;
use crate::operation::OperationEncoded;

#[derive(Debug, Clone)]
pub struct EntryWithOperation(EntrySigned, Option<OperationEncoded>);

impl EntryWithOperation {
    pub fn new(
        entry: EntrySigned,
        operation: Option<OperationEncoded>,
    ) -> Result<Self, StorageProviderError> {
        // TODO: Validate entry + operation here

        Ok(Self(entry, operation))
    }
    pub fn entry_encoded(&self) -> &EntrySigned {
        &self.0
    }
    pub fn operation_encoded(&self) -> Option<&OperationEncoded> {
        self.1.as_ref()
    }
}
