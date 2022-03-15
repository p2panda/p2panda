// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::EntrySigned;
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::storage_provider::StorageProviderError;
use crate::Validate;

/// Request body of `panda_getEntryArguments`.
pub trait AsEntryArgsRequest {
    fn author(&self) -> &Author;
    fn document(&self) -> &Option<Hash>;
    fn validate(&self) -> Result<(), StorageProviderError> {
        // Validate `author` request parameter
        self.author().validate()?;

        // Validate `document` request parameter when it is set
        let document = match self.document() {
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
    fn entry_encoded(&self) -> &EntrySigned;
    fn operation_encoded(&self) -> Option<&OperationEncoded>;
}
