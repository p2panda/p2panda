// SPDX-License-Identifier: AGPL-3.0-or-later
use std::fmt::Debug;

use super::StorageProviderError;
use crate::entry::{decode_entry, EntrySigned};
use crate::operation::OperationEncoded;
use crate::Validate;

/// Struct wrapping an entry with it's operation.
///
/// Used internally throughout `storage_provider` in method args and default trait definitions.
/// The `AsStorageEntry` trait requires `TryFrom<EntryWithOperation>` & `TryInto<EntryWithOperation>`
/// conversion traits to be present.
#[derive(Debug, Clone)]
pub struct EntryWithOperation(EntrySigned, OperationEncoded);

impl EntryWithOperation {
    /// Instantiate a new EntryWithOperation.
    pub fn new(
        entry: EntrySigned,
        operation: OperationEncoded,
    ) -> Result<Self, StorageProviderError> {
        // TODO: Validate entry + operation here
        let entry_with_operation = Self(entry, operation);
        entry_with_operation.validate()?;
        Ok(entry_with_operation)
    }

    /// Returns a reference to the encoded entry.
    pub fn entry_encoded(&self) -> &EntrySigned {
        &self.0
    }

    /// Returns a refernce to the optional encoded operation.
    pub fn operation_encoded(&self) -> &OperationEncoded {
        &self.1
    }
}

impl Validate for EntryWithOperation {
    type Error = StorageProviderError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.entry_encoded().validate()?;
        self.operation_encoded().validate()?;
        decode_entry(self.entry_encoded(), Some(self.operation_encoded()))?;
        Ok(())
    }
}
