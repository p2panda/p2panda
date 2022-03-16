// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::EntrySigned;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::storage_provider::StorageProviderError;
use crate::Validate;

/// Request body of `panda_getEntryArguments`.
pub trait AsEntryArgsRequest {
    /// Returns the Author parameter.
    fn author(&self) -> &Author;
    /// Returns the document id Hash parameter.
    ///
    /// TODO: Needs updating once we use `DocumentId` here.
    fn document(&self) -> &Option<DocumentId>;
    /// Validates the `EntryArgument` parameters
    fn validate(&self) -> Result<(), StorageProviderError> {
        // Validate `author` request parameter
        self.author().validate()?;

        // Validate `document` request parameter when it is set
        match self.document() {
            Some(doc) => {
                doc.validate()?;
                Some(doc)
            }
            None => None,
        };
        Ok(())
    }
}

/// Request body of `panda_publishEntry`.
pub trait AsPublishEntryRequest {
    /// Returns the EntrySigned parameter
    fn entry_encoded(&self) -> &EntrySigned;
    /// Returns the OperationEncoded parameter
    ///
    /// Currently not optional.
    fn operation_encoded(&self) -> &OperationEncoded;
}
