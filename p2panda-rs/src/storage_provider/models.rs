// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt::Debug;

use super::StorageProviderError;
use crate::entry::EntrySigned;
use crate::operation::OperationEncoded;

/// Struct wrapping an entry with it's operation.
///
/// Used internally throughout `storage_provider` in method args and default trait definitions.
/// The `AsStorageEntry` trait requires `TryFrom<EntryWithOperation>` & `TryInto<EntryWithOperation>`
/// conversion traits to be present.
#[derive(Debug, Clone)]
pub struct EntryWithOperation(EntrySigned, Option<OperationEncoded>);

impl EntryWithOperation {
    /// Instantiate a new EntryWithOperation.
    pub fn new(
        entry: EntrySigned,
        operation: Option<OperationEncoded>,
    ) -> Result<Self, StorageProviderError> {
        // TODO: Validate entry + operation here

        Ok(Self(entry, operation))
    }

    /// Returns a reference to the encoded entry.
    pub fn entry_encoded(&self) -> &EntrySigned {
        &self.0
    }

    /// Returns a refernce to the optional encoded operation.
    pub fn operation_encoded(&self) -> Option<&OperationEncoded> {
        self.1.as_ref()
    }
}
