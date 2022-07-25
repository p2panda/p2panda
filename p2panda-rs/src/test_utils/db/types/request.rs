// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::{decode_entry, EncodedEntry};
use crate::identity::Author;
use crate::operation::EncodedOperation;
use crate::storage_provider::traits::{AsEntryArgsRequest, AsPublishEntryRequest};
use crate::storage_provider::ValidationError;
use crate::Validate;

/// Arguments for a request to publish an entry on a p2panda node.
///
/// In this case the encoded operation is a mandatory argumanet.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishEntryRequest {
    /// The encoded entry.
    pub entry: EncodedEntry,

    /// The encoded operation.
    pub operation: EncodedOperation,
}

impl AsPublishEntryRequest for PublishEntryRequest {
    fn entry_signed(&self) -> &EncodedEntry {
        &self.entry
    }

    fn operation_encoded(&self) -> &EncodedOperation {
        &self.operation
    }
}

/// Arguments for requesting next entry arguments for an author and optionally document.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryArgsRequest {
    /// The author you will be publishing an entry with.
    pub public_key: Author,

    /// The id of the document you will be updating.
    ///
    /// If not included, it is assumed we are creating a new document.
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
