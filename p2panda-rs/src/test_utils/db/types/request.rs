// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::{decode_entry, EntrySigned};
use crate::identity::Author;
use crate::operation::OperationEncoded;
use crate::storage_provider::traits::{AsEntryArgsRequest, AsPublishEntryRequest};
use crate::storage_provider::ValidationError;
use crate::Validate;

#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryRequest {
    pub entry: EntrySigned,
    pub operation: OperationEncoded,
}

impl AsPublishEntryRequest for PublishEntryRequest {
    fn entry_signed(&self) -> &EntrySigned {
        &self.entry
    }

    fn operation_encoded(&self) -> &OperationEncoded {
        &self.operation
    }
}

impl Validate for PublishEntryRequest {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.entry_signed().validate()?;
        self.operation_encoded().validate()?;
        decode_entry(self.entry_signed(), Some(self.operation_encoded()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryArgsRequest {
    pub public_key: Author,
    pub document_id: Option<DocumentId>,
}

impl AsEntryArgsRequest for EntryArgsRequest {
    fn author(&self) -> &Author {
        &self.public_key
    }
    fn document_id(&self) -> &Option<DocumentId> {
        &self.document_id
    }
}

impl Validate for EntryArgsRequest {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Validate `author` request parameter
        self.author().validate()?;

        // Validate `document` request parameter when it is set
        match self.document_id() {
            None => (),
            Some(doc) => {
                doc.validate()?;
            }
        };
        Ok(())
    }
}
